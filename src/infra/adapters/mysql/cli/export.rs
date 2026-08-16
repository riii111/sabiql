use std::ffi::OsStr;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use tokio::io::{AsyncRead, BufReader, ReadBuf};
use uuid::Uuid;

use crate::adapters::csv_export::CsvFileWriter;
use crate::app::policy::sql::mysql_statement::classify_mysql_statement;
use crate::app::ports::outbound::{AccessMode, DbOperationError};

use super::super::{dsn::MySqlDsn, option_file::MySqlOptionFile};
use super::error::{classify_mysql_query_failure, has_mysql_cli_error, validate_mode_probe};
#[cfg(not(unix))]
use super::pipe::MySqlExportPipeSource;
use super::policy::mysql_metadata_fallback_kind;
use super::process::{
    MYSQL_QUERY_TIMEOUT, MySqlProcess, configure_mysql_session, finish_mysql_session,
    mysql_metadata_columns, read_one_mysql_resultset, run_mysql_process_with_timeout,
    write_mysql_statement,
};
#[cfg(unix)]
use super::pty::MySqlExportPtySource;
use super::xml::{MySqlField, decode_mysql_xml_reference, parse_mysql_field, parse_mysql_xml};

const MYSQL_EXPORT_TIMEOUT: Duration = Duration::from_secs(MYSQL_QUERY_TIMEOUT.as_secs() * 10);
const MYSQL_CSV_MAX_FIELD_BYTES: usize = 16 * 1024 * 1024;
const MYSQL_CSV_FIELD_LIMIT_ERROR: &str = "MySQL CSV field exceeds the 16777216-byte limit";

#[derive(Clone, Copy)]
enum MySqlXmlFieldState {
    Outside,
    FieldStartPending,
    FieldStartTag,
    FieldContent,
    Cdata,
}

struct MySqlXmlFieldLimitReader<R> {
    inner: R,
    state: MySqlXmlFieldState,
    field_bytes: usize,
    window: Vec<u8>,
    limit_error_pending: bool,
}

impl<R> MySqlXmlFieldLimitReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            state: MySqlXmlFieldState::Outside,
            field_bytes: 0,
            window: Vec::with_capacity(9),
            limit_error_pending: false,
        }
    }

    fn into_inner(self) -> R {
        self.inner
    }

    fn process(&mut self, bytes: &[u8]) -> usize {
        for (index, &byte) in bytes.iter().enumerate() {
            match self.state {
                MySqlXmlFieldState::FieldStartTag => {
                    if byte == b'>' {
                        self.state = if self.window.ends_with(b"/>") {
                            self.field_bytes = 0;
                            MySqlXmlFieldState::Outside
                        } else {
                            MySqlXmlFieldState::FieldContent
                        };
                    }
                }
                MySqlXmlFieldState::FieldContent | MySqlXmlFieldState::Cdata => {
                    if self.field_bytes >= MYSQL_CSV_MAX_FIELD_BYTES {
                        return index;
                    }
                    self.field_bytes += 1;
                }
                MySqlXmlFieldState::FieldStartPending => {
                    if byte.is_ascii_whitespace() || matches!(byte, b'>' | b'/') {
                        self.state = if byte == b'>' {
                            MySqlXmlFieldState::FieldContent
                        } else {
                            MySqlXmlFieldState::FieldStartTag
                        };
                    } else {
                        self.state = MySqlXmlFieldState::Outside;
                    }
                }
                MySqlXmlFieldState::Outside => {}
            }

            self.window.push(byte);
            if self.window.len() > 9 {
                self.window.remove(0);
            }

            match self.state {
                MySqlXmlFieldState::FieldStartTag => {}
                MySqlXmlFieldState::FieldContent => {
                    if self.window.ends_with(b"<![CDATA[") {
                        self.state = MySqlXmlFieldState::Cdata;
                    } else if self.window.ends_with(b"</field>") {
                        self.state = MySqlXmlFieldState::Outside;
                        self.field_bytes = 0;
                    }
                }
                MySqlXmlFieldState::Cdata => {
                    if self.window.ends_with(b"]]>") {
                        self.state = MySqlXmlFieldState::FieldContent;
                    }
                }
                MySqlXmlFieldState::FieldStartPending | MySqlXmlFieldState::Outside => {
                    if self.window.ends_with(b"<field") {
                        self.state = MySqlXmlFieldState::FieldStartPending;
                    }
                }
            }
        }
        bytes.len()
    }
}

impl<R> AsyncRead for MySqlXmlFieldLimitReader<R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.limit_error_pending {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                MYSQL_CSV_FIELD_LIMIT_ERROR,
            )));
        }
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        let mut chunk = [0; 8192];
        let chunk_len = buf.remaining().min(chunk.len());
        let mut chunk_buf = ReadBuf::new(&mut chunk[..chunk_len]);
        match Pin::new(&mut self.inner).poll_read(cx, &mut chunk_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let bytes_read = chunk_buf.filled().len();
                if bytes_read == 0 {
                    return Poll::Ready(Ok(()));
                }
                let bytes_allowed = self.process(&chunk[..bytes_read]);
                if bytes_allowed < bytes_read {
                    self.limit_error_pending = true;
                }
                if bytes_allowed > 0 {
                    buf.put_slice(&chunk[..bytes_allowed]);
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        MYSQL_CSV_FIELD_LIMIT_ERROR,
                    )))
                }
            }
        }
    }
}

fn append_csv_field_value(field: &mut MySqlField, value: &str) -> Result<(), DbOperationError> {
    if field.value.len().saturating_add(value.len()) > MYSQL_CSV_MAX_FIELD_BYTES {
        return Err(DbOperationError::QueryFailed(
            MYSQL_CSV_FIELD_LIMIT_ERROR.to_string(),
        ));
    }
    field.value.push_str(value);
    Ok(())
}

pub(in crate::adapters::mysql) async fn export_mysql_csv_to_file(
    target: MySqlDsn,
    query: &str,
    path: PathBuf,
) -> Result<(), DbOperationError> {
    let option_file = MySqlOptionFile::create(&target)?;
    let mut process = MySqlProcess::spawn_with_program(OsStr::new("mysql"), &option_file.path)?;
    run_mysql_process_with_timeout(MYSQL_EXPORT_TIMEOUT, &mut process, async |process| {
        run_mysql_export_process(process, &option_file.path, query, path).await
    })
    .await
}

pub(super) async fn run_mysql_export_process(
    process: &mut MySqlProcess,
    option_file: &std::path::Path,
    query: &str,
    path: PathBuf,
) -> Result<(), DbOperationError> {
    let marker = Uuid::new_v4().simple().to_string();
    let probe_query =
        format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
    write_mysql_statement(process, &probe_query).await?;
    let probe_xml = read_one_mysql_resultset(process).await?;
    let probe = parse_mysql_xml(&probe_xml)?;
    validate_mode_probe(&probe, &marker)?;
    configure_mysql_session(process, AccessMode::ReadOnly).await?;

    write_mysql_statement(process, query).await?;
    let mut csv_writer = CsvFileWriter::create(path).await?;
    let columns = stream_mysql_resultset_to_csv(process, &mut csv_writer).await?;
    if columns.is_none() {
        let statement = classify_mysql_statement(query)
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        let fallback_kind = mysql_metadata_fallback_kind(&statement.kind).ok_or_else(|| {
            DbOperationError::QueryFailed(
                "MySQL empty CSV result has no supported metadata fallback".to_string(),
            )
        })?;
        let columns = mysql_metadata_columns(
            process,
            option_file,
            query,
            fallback_kind,
            AccessMode::ReadOnly,
        )
        .await?;
        csv_writer.write_record(columns.iter()).await?;
    }

    let result = finish_mysql_session(process).await?;
    validate_mysql_export_exit(result.status, result.forcibly_stopped, &result.error_bytes)?;

    csv_writer.finish().await
}

fn validate_mysql_export_exit(
    status: std::process::ExitStatus,
    forcibly_stopped: bool,
    error_bytes: &[u8],
) -> Result<(), DbOperationError> {
    if has_mysql_cli_error(error_bytes) {
        return Err(classify_mysql_query_failure(error_bytes));
    }
    if !status.success() && !forcibly_stopped {
        return Err(classify_mysql_query_failure(error_bytes));
    }
    Ok(())
}

pub(super) async fn stream_mysql_resultset_to_csv(
    process: &mut MySqlProcess,
    csv_writer: &mut CsvFileWriter,
) -> Result<Option<Vec<String>>, DbOperationError> {
    #[cfg(unix)]
    {
        let source = MySqlExportPtySource {
            pty: &mut process.pty,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            pending: Vec::new(),
            frame_scanner: super::xml::MySqlResultsetFrameScanner::default(),
            started: false,
        };
        let source = MySqlXmlFieldLimitReader::new(source);
        let mut reader = Reader::from_reader(BufReader::new(source));
        reader.config_mut().trim_text(false);
        let result = stream_mysql_xml_to_csv(&mut reader, csv_writer).await;
        let buffered = reader.into_inner();
        let unread = buffered.buffer().to_vec();
        let source = buffered.into_inner().into_inner();
        source.pty.pending.extend(unread);
        source.pty.pending.extend(source.pending);
        if has_mysql_cli_error(&source.error_output) {
            return Err(classify_mysql_query_failure(&source.error_output));
        }
        result
    }

    #[cfg(not(unix))]
    {
        let mut source = MySqlExportPipeSource {
            stdout: &mut process.stdout,
            stderr: &mut process.stderr,
            pending: &mut process.pending,
            error_output: Vec::new(),
            error_buffer: Vec::new(),
            stderr_buffer: [0; 4096],
            stderr_closed: false,
            stdout_closed: false,
        };
        let pending_stderr = std::mem::take(&mut process.pending_stderr);
        source.capture_error(&pending_stderr);
        let source = MySqlXmlFieldLimitReader::new(source);
        let mut reader = Reader::from_reader(BufReader::new(source));
        reader.config_mut().trim_text(false);
        let result = stream_mysql_xml_to_csv(&mut reader, csv_writer).await;
        let buffered = reader.into_inner();
        let unread = buffered.buffer().to_vec();
        let source = buffered.into_inner().into_inner();
        source.pending.extend(unread);
        if source.error_output.is_empty() {
            process
                .pending_stderr
                .extend_from_slice(&source.error_buffer);
        }
        if has_mysql_cli_error(&source.error_output) {
            return Err(classify_mysql_query_failure(&source.error_output));
        }
        result
    }
}

async fn stream_mysql_xml_to_csv<R>(
    reader: &mut Reader<BufReader<R>>,
    csv_writer: &mut CsvFileWriter,
) -> Result<Option<Vec<String>>, DbOperationError>
where
    R: AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    let mut resultset_count = 0;
    let mut in_resultset = false;
    let mut current_row: Option<Vec<(String, String)>> = None;
    let mut current_field: Option<MySqlField> = None;
    let mut columns: Option<Vec<String>> = None;

    loop {
        let event = reader
            .read_event_into_async(&mut buffer)
            .await
            .map_err(|error| {
                DbOperationError::QueryFailed(format!("invalid MySQL XML result: {error}"))
            })?;
        match event {
            Event::Start(element) => match element.name().as_ref() {
                b"resultset" => {
                    if in_resultset || resultset_count > 0 {
                        return Err(DbOperationError::QueryFailed(
                            "mysql returned more than one resultset".to_string(),
                        ));
                    }
                    resultset_count += 1;
                    in_resultset = true;
                }
                b"row" if in_resultset && current_row.is_none() => {
                    current_row = Some(Vec::new());
                }
                b"field" if current_row.is_some() && current_field.is_none() => {
                    current_field = Some(parse_mysql_field(&element)?);
                }
                _ => {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected element in MySQL XML result".to_string(),
                    ));
                }
            },
            Event::Empty(element) if element.name().as_ref() == b"field" => {
                let row = current_row.as_mut().ok_or_else(|| {
                    DbOperationError::QueryFailed("MySQL XML field is outside a row".to_string())
                })?;
                if current_field.is_some() {
                    return Err(DbOperationError::QueryFailed(
                        "nested MySQL XML fields are not supported".to_string(),
                    ));
                }
                row.push(parse_mysql_field(&element)?.finish_raw());
            }
            Event::Text(text) => {
                let decoded = text.decode().map_err(|error| {
                    DbOperationError::QueryFailed(format!("invalid MySQL XML text: {error}"))
                })?;
                let text = unescape(&decoded).map_err(|error| {
                    DbOperationError::QueryFailed(format!("invalid MySQL XML text: {error}"))
                })?;
                if let Some(field) = current_field.as_mut() {
                    append_csv_field_value(field, &text)?;
                } else if !text.chars().all(char::is_whitespace) {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected text in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::GeneralRef(reference) => {
                let text = decode_mysql_xml_reference(&reference)?;
                if let Some(field) = current_field.as_mut() {
                    append_csv_field_value(field, &text)?;
                } else if !text.chars().all(char::is_whitespace) {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected text in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::CData(data) => {
                if let Some(field) = current_field.as_mut() {
                    let text = std::str::from_utf8(data.as_ref()).map_err(|error| {
                        DbOperationError::QueryFailed(format!("invalid MySQL XML text: {error}"))
                    })?;
                    append_csv_field_value(field, text)?;
                } else {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected CDATA in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::End(element) => match element.name().as_ref() {
                b"field" => {
                    let row = current_row.as_mut().ok_or_else(|| {
                        DbOperationError::QueryFailed(
                            "MySQL XML field is outside a row".to_string(),
                        )
                    })?;
                    let field = current_field.take().ok_or_else(|| {
                        DbOperationError::QueryFailed("unexpected MySQL XML field end".to_string())
                    })?;
                    row.push(field.finish_raw());
                }
                b"row" => {
                    let row = current_row.take().ok_or_else(|| {
                        DbOperationError::QueryFailed("unexpected MySQL XML row end".to_string())
                    })?;
                    let row_columns = row.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
                    if let Some(columns) = columns.as_ref() {
                        if row_columns != *columns {
                            return Err(DbOperationError::QueryFailed(
                                "MySQL XML rows have inconsistent fields".to_string(),
                            ));
                        }
                    } else {
                        csv_writer.write_record(row_columns.iter()).await?;
                        columns = Some(row_columns);
                    }
                    csv_writer
                        .write_record(row.iter().map(|(_, value)| value))
                        .await?;
                }
                b"resultset" => {
                    if !in_resultset || current_row.is_some() || current_field.is_some() {
                        return Err(DbOperationError::QueryFailed(
                            "malformed MySQL XML resultset".to_string(),
                        ));
                    }
                    return Ok(columns);
                }
                _ => {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected MySQL XML closing element".to_string(),
                    ));
                }
            },
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Eof => break,
            _ => {
                return Err(DbOperationError::QueryFailed(
                    "unexpected event in MySQL XML result".to_string(),
                ));
            }
        }
        buffer.clear();
    }

    if resultset_count != 1 || in_resultset || current_row.is_some() || current_field.is_some() {
        return Err(DbOperationError::QueryFailed(
            "MySQL XML result did not contain one complete resultset".to_string(),
        ));
    }
    Ok(columns)
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    use tokio::io::AsyncWriteExt;

    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn streams_mysql_xml_rows_into_csv_without_binary_type_inference() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row>
<field name="comma">a,b</field>
<field name="quote">a&quot;b</field>
<field name="newline"><![CDATA[line1
line2]]></field>
<field name="tab">a	b</field>
<field name="unicode">日本語</field>
<field name="null" xsi:nil="true"/>
<field name="empty"></field>
<field name="binary">0x00FF</field>
</row></resultset>"#;
        let (mut input, output) = tokio::io::duplex(32);
        let producer = tokio::spawn(async move {
            input.write_all(xml.as_bytes()).await.unwrap();
        });
        let directory = tempdir().unwrap();
        let path = directory.path().join("stream.csv");
        let mut csv_writer = CsvFileWriter::create(path.clone()).await.unwrap();
        let mut reader = Reader::from_reader(BufReader::new(MySqlXmlFieldLimitReader::new(output)));
        reader.config_mut().trim_text(false);

        stream_mysql_xml_to_csv(&mut reader, &mut csv_writer)
            .await
            .unwrap();
        csv_writer.finish().await.unwrap();
        producer.await.unwrap();

        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "comma,quote,newline,tab,unicode,null,empty,binary\n\
             \"a,b\",\"a\"\"b\",\"line1\n\
             line2\",a\tb,日本語,,,0x00FF\n"
                .replace("             ", "")
        );
    }

    #[tokio::test]
    async fn rejects_mysql_csv_field_over_byte_limit() {
        let value = "x".repeat(MYSQL_CSV_MAX_FIELD_BYTES + 1);
        let xml =
            format!("<resultset><row><field name=\"payload\">{value}</field></row></resultset>");
        let (mut input, output) = tokio::io::duplex(32);
        let producer = tokio::spawn(async move {
            input.write_all(xml.as_bytes()).await.unwrap();
        });
        let directory = tempdir().unwrap();
        let path = directory.path().join("oversized-field.csv");
        let mut csv_writer = CsvFileWriter::create(path).await.unwrap();
        let mut reader = Reader::from_reader(BufReader::new(output));
        reader.config_mut().trim_text(false);

        let error = stream_mysql_xml_to_csv(&mut reader, &mut csv_writer)
            .await
            .unwrap_err();
        producer.await.unwrap();

        assert!(matches!(&error, DbOperationError::QueryFailed(_)));
        assert!(error.masked_details().contains(MYSQL_CSV_FIELD_LIMIT_ERROR));
    }

    #[tokio::test]
    async fn stops_reading_mysql_csv_field_at_byte_limit() {
        let value = "x".repeat(MYSQL_CSV_MAX_FIELD_BYTES + 1);
        let xml =
            format!("<resultset><row><field name=\"payload\">{value}</field></row></resultset>");
        let mut source = MySqlXmlFieldLimitReader::new(std::io::Cursor::new(xml.as_bytes()));
        let mut output = Vec::new();

        let error = tokio::io::AsyncReadExt::read_to_end(&mut source, &mut output)
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), MYSQL_CSV_FIELD_LIMIT_ERROR);
        assert!(output.len() < xml.len());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_cli_error_after_forced_stop() {
        let result = validate_mysql_export_exit(
            std::process::ExitStatus::from_raw(9),
            true,
            b"ERROR 1054 (42S02): Unknown column missing_column",
        );

        assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
    }
}
