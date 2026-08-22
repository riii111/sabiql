use std::ffi::OsStr;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::adapters::csv_export::CsvFileWriter;
use crate::app::ports::outbound::{AccessMode, DbOperationError};
use crate::domain::{RefreshScope, mysql_sql::classify_mysql_statement};
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use tokio::io::{AsyncRead, BufReader, ReadBuf};

use super::super::{dsn::MySqlDsn, option_file::MySqlOptionFile};
use super::error::{classify_mysql_query_failure_with_packet_limit, has_mysql_cli_error};
#[cfg(not(unix))]
use super::pipe::MySqlExportPipeSource;
use super::policy::mysql_metadata_fallback_kind;
use super::process::{
    MYSQL_QUERY_TIMEOUT, MySqlProcess, configure_mysql_session, finish_mysql_session,
    mysql_metadata_columns, run_mysql_process_with_timeout, validate_mysql_session_exit,
    write_mysql_statement,
};
#[cfg(unix)]
use super::pty::MySqlExportPtySource;
use super::xml::{MySqlField, decode_mysql_xml_reference, parse_mysql_field};

const MYSQL_EXPORT_TIMEOUT: Duration = Duration::from_secs(MYSQL_QUERY_TIMEOUT.as_secs() * 10);
const MYSQL_CSV_MAX_DECODED_FIELD_BYTES: usize = 16 * 1024 * 1024;
const MYSQL_CSV_MAX_DECODED_ROW_BYTES: usize = MYSQL_CSV_MAX_DECODED_FIELD_BYTES;
const MYSQL_CSV_DECODED_FIELD_LIMIT_ERROR: &str =
    "MySQL decoded CSV field exceeds 16 MiB (16777216 bytes)";
const MYSQL_CSV_DECODED_ROW_LIMIT_ERROR: &str =
    "MySQL decoded CSV row exceeds 16 MiB (16777216 bytes)";

#[derive(Clone, Copy)]
enum MySqlXmlFieldState {
    Outside,
    FieldStartCandidate,
    FieldStartPending,
    FieldStartTag,
    FieldContent,
    Cdata,
}

#[derive(Clone, Copy)]
enum MySqlXmlPendingKind {
    Markup,
    Entity,
}

struct MySqlXmlFieldLimitReader<R> {
    inner: R,
    state: MySqlXmlFieldState,
    field_bytes: usize,
    field_start_match: usize,
    field_start_quote: Option<u8>,
    field_start_self_closing: bool,
    pending: Vec<u8>,
    pending_kind: Option<MySqlXmlPendingKind>,
    limit_error_pending: bool,
}

impl<R> MySqlXmlFieldLimitReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            state: MySqlXmlFieldState::Outside,
            field_bytes: 0,
            field_start_match: 0,
            field_start_quote: None,
            field_start_self_closing: false,
            pending: Vec::new(),
            pending_kind: None,
            limit_error_pending: false,
        }
    }

    fn into_inner(self) -> R {
        self.inner
    }

    fn count_field_bytes(&mut self, bytes: usize) -> bool {
        if self.field_bytes > MYSQL_CSV_MAX_DECODED_FIELD_BYTES.saturating_sub(bytes) {
            false
        } else {
            self.field_bytes += bytes;
            true
        }
    }

    fn process_pending_byte(&mut self, byte: u8) -> bool {
        self.pending.push(byte);
        match self.pending_kind {
            Some(MySqlXmlPendingKind::Markup) => {
                let target = match self.state {
                    MySqlXmlFieldState::FieldContent => {
                        if self.pending.first() == Some(&b'<') && self.pending.get(1) == Some(&b'!')
                        {
                            b"<![CDATA[".as_slice()
                        } else {
                            b"</field>".as_slice()
                        }
                    }
                    MySqlXmlFieldState::Cdata => b"]]>".as_slice(),
                    _ => unreachable!(),
                };
                if target.starts_with(&self.pending) {
                    if self.pending == target {
                        let state = self.state;
                        self.pending.clear();
                        self.pending_kind = None;
                        self.state = match state {
                            MySqlXmlFieldState::FieldContent => {
                                if target == b"<![CDATA[" {
                                    MySqlXmlFieldState::Cdata
                                } else {
                                    self.field_bytes = 0;
                                    MySqlXmlFieldState::Outside
                                }
                            }
                            MySqlXmlFieldState::Cdata => MySqlXmlFieldState::FieldContent,
                            _ => unreachable!(),
                        };
                    }
                    true
                } else {
                    let keep = (1..target.len().min(self.pending.len()))
                        .rev()
                        .find(|&length| self.pending.ends_with(&target[..length]))
                        .unwrap_or(0);
                    let count = self.pending.len() - keep;
                    if !self.count_field_bytes(count) {
                        return false;
                    }
                    if keep == 0 {
                        self.pending.clear();
                        self.pending_kind = None;
                    } else {
                        let start = self.pending.len() - keep;
                        self.pending.copy_within(start.., 0);
                        self.pending.truncate(keep);
                    }
                    true
                }
            }
            Some(MySqlXmlPendingKind::Entity) => {
                if byte == b';' {
                    let decoded_len = std::str::from_utf8(&self.pending)
                        .ok()
                        .and_then(|entity| unescape(entity).ok())
                        .map_or(self.pending.len(), |decoded| decoded.len());
                    self.pending.clear();
                    self.pending_kind = None;
                    self.count_field_bytes(decoded_len)
                } else if self.pending.len() >= 64 {
                    let pending_len = self.pending.len();
                    self.pending.clear();
                    self.pending_kind = None;
                    self.count_field_bytes(pending_len)
                } else {
                    true
                }
            }
            None => unreachable!(),
        }
    }

    fn process_byte(&mut self, byte: u8) -> bool {
        if self.pending_kind.is_some() {
            return self.process_pending_byte(byte);
        }

        match self.state {
            MySqlXmlFieldState::Outside => {
                if byte == b'<' {
                    self.state = MySqlXmlFieldState::FieldStartCandidate;
                    self.field_start_match = 0;
                }
                true
            }
            MySqlXmlFieldState::FieldStartCandidate => {
                let field_name = b"field";
                if byte == field_name[self.field_start_match] {
                    self.field_start_match += 1;
                    if self.field_start_match == field_name.len() {
                        self.state = MySqlXmlFieldState::FieldStartPending;
                    }
                } else {
                    self.state = if byte == b'<' {
                        self.field_start_match = 0;
                        MySqlXmlFieldState::FieldStartCandidate
                    } else {
                        MySqlXmlFieldState::Outside
                    };
                }
                true
            }
            MySqlXmlFieldState::FieldStartPending => {
                if byte.is_ascii_whitespace() {
                    self.state = MySqlXmlFieldState::FieldStartTag;
                    self.field_start_quote = None;
                    self.field_start_self_closing = false;
                } else if byte == b'>' {
                    self.field_bytes = 0;
                    self.state = MySqlXmlFieldState::FieldContent;
                } else if byte == b'/' {
                    self.state = MySqlXmlFieldState::FieldStartTag;
                    self.field_start_quote = None;
                    self.field_start_self_closing = true;
                } else {
                    self.state = MySqlXmlFieldState::Outside;
                }
                true
            }
            MySqlXmlFieldState::FieldStartTag => {
                if let Some(quote) = self.field_start_quote {
                    if byte == quote {
                        self.field_start_quote = None;
                    }
                } else if matches!(byte, b'\'' | b'"') {
                    self.field_start_quote = Some(byte);
                } else if byte == b'/' {
                    self.field_start_self_closing = true;
                } else if byte == b'>' {
                    self.field_bytes = 0;
                    self.state = if self.field_start_self_closing {
                        MySqlXmlFieldState::Outside
                    } else {
                        MySqlXmlFieldState::FieldContent
                    };
                } else if !byte.is_ascii_whitespace() {
                    self.field_start_self_closing = false;
                }
                true
            }
            MySqlXmlFieldState::FieldContent => {
                if byte == b'<' {
                    self.pending.push(byte);
                    self.pending_kind = Some(MySqlXmlPendingKind::Markup);
                    true
                } else if byte == b'&' {
                    self.pending.push(byte);
                    self.pending_kind = Some(MySqlXmlPendingKind::Entity);
                    true
                } else {
                    self.count_field_bytes(1)
                }
            }
            MySqlXmlFieldState::Cdata => {
                if byte == b']' {
                    self.pending.push(byte);
                    self.pending_kind = Some(MySqlXmlPendingKind::Markup);
                    true
                } else {
                    self.count_field_bytes(1)
                }
            }
        }
    }

    fn process(&mut self, bytes: &[u8]) -> usize {
        for (index, &byte) in bytes.iter().enumerate() {
            if !self.process_byte(byte) {
                return index;
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
                MYSQL_CSV_DECODED_FIELD_LIMIT_ERROR,
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
                        MYSQL_CSV_DECODED_FIELD_LIMIT_ERROR,
                    )))
                }
            }
        }
    }
}

fn append_csv_field_value(
    field: &mut MySqlField,
    row_bytes: &mut usize,
    value: &str,
) -> Result<(), DbOperationError> {
    if field.value.len().saturating_add(value.len()) > MYSQL_CSV_MAX_DECODED_FIELD_BYTES {
        return Err(DbOperationError::QueryFailed(
            MYSQL_CSV_DECODED_FIELD_LIMIT_ERROR.to_string(),
        ));
    }

    let next_row_bytes = row_bytes.saturating_add(value.len());
    if next_row_bytes > MYSQL_CSV_MAX_DECODED_ROW_BYTES {
        return Err(DbOperationError::QueryFailed(
            MYSQL_CSV_DECODED_ROW_LIMIT_ERROR.to_string(),
        ));
    }

    field.value.push_str(value);
    *row_bytes = next_row_bytes;
    Ok(())
}

pub(in crate::adapters::mysql) async fn export_mysql_csv_to_file(
    target: MySqlDsn,
    query: &str,
    path: PathBuf,
) -> Result<(), DbOperationError> {
    let option_file = MySqlOptionFile::create(&target)?;
    let mut process = MySqlProcess::spawn_with_program(OsStr::new("mysql"), &option_file.path)?;
    run_mysql_process_with_timeout(
        MYSQL_EXPORT_TIMEOUT,
        &mut process,
        RefreshScope::None,
        async |process| run_mysql_export_process(process, &option_file.path, query, path).await,
    )
    .await
}

pub(super) async fn run_mysql_export_process(
    process: &mut MySqlProcess,
    option_file: &std::path::Path,
    query: &str,
    path: PathBuf,
) -> Result<(), DbOperationError> {
    configure_mysql_session(process, AccessMode::ReadOnly).await?;
    process.probe_sql_mode().await?;

    write_mysql_statement(process, query).await?;
    let mut csv_writer = CsvFileWriter::create(path).await?;
    let columns = stream_mysql_resultset_to_csv(process, &mut csv_writer).await?;
    if columns.is_none() {
        let statement = classify_mysql_statement(query)
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        let fallback_kind = mysql_metadata_fallback_kind(statement.kind()).ok_or_else(|| {
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
    validate_mysql_session_exit(&result, process.client_packet_limit_bytes)?;

    csv_writer.finish().await
}

pub(super) async fn stream_mysql_resultset_to_csv(
    process: &mut MySqlProcess,
    csv_writer: &mut CsvFileWriter,
) -> Result<Option<Vec<String>>, DbOperationError> {
    #[cfg(unix)]
    {
        let source = MySqlExportPtySource {
            pty: &mut process.pty,
            client_packet_limit_bytes: process.client_packet_limit_bytes,
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
            return Err(classify_mysql_query_failure_with_packet_limit(
                &source.error_output,
                source.client_packet_limit_bytes,
            ));
        }
        result
    }

    #[cfg(not(unix))]
    {
        let mut source = MySqlExportPipeSource {
            stdout: &mut process.stdout,
            stderr: &mut process.stderr,
            pending: &mut process.pending,
            client_packet_limit_bytes: process.client_packet_limit_bytes,
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
            return Err(classify_mysql_query_failure_with_packet_limit(
                &source.error_output,
                source.client_packet_limit_bytes,
            ));
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
    let mut current_row_bytes = 0;
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
                    current_row_bytes = 0;
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
                    append_csv_field_value(field, &mut current_row_bytes, &text)?;
                } else if !text.chars().all(char::is_whitespace) {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected text in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::GeneralRef(reference) => {
                let text = decode_mysql_xml_reference(&reference)?;
                if let Some(field) = current_field.as_mut() {
                    append_csv_field_value(field, &mut current_row_bytes, &text)?;
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
                    append_csv_field_value(field, &mut current_row_bytes, text)?;
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

    use crate::adapters::csv_export::export_to_path;
    use tokio::io::AsyncWriteExt;

    use super::*;
    use tempfile::tempdir;

    fn xml_row(fields: &[(&str, &str)]) -> String {
        let mut xml = String::from("<row>");
        for (name, value) in fields {
            xml.push_str("<field name=\"");
            xml.push_str(name);
            xml.push_str("\">");
            xml.push_str(value);
            xml.push_str("</field>");
        }
        xml.push_str("</row>");
        xml
    }

    fn xml_resultset(rows: &[String]) -> String {
        let mut xml = String::from("<resultset>");
        for row in rows {
            xml.push_str(row);
        }
        xml.push_str("</resultset>");
        xml
    }

    async fn stream_xml_to_csv_file(
        xml: &str,
        path: std::path::PathBuf,
    ) -> Result<(), DbOperationError> {
        let mut csv_writer = CsvFileWriter::create(path).await?;
        let mut reader = Reader::from_reader(BufReader::new(std::io::Cursor::new(xml.as_bytes())));
        reader.config_mut().trim_text(false);
        stream_mysql_xml_to_csv(&mut reader, &mut csv_writer).await?;
        csv_writer.finish().await
    }

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
        let value = "x".repeat(MYSQL_CSV_MAX_DECODED_FIELD_BYTES + 1);
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
        assert!(
            error
                .masked_details()
                .contains(MYSQL_CSV_DECODED_FIELD_LIMIT_ERROR)
        );
    }

    #[tokio::test]
    async fn accepts_mysql_csv_row_just_below_byte_limit() {
        let first = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES / 2);
        let second = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES - first.len() - 1);
        let xml = xml_resultset(&[xml_row(&[
            ("first", first.as_str()),
            ("second", second.as_str()),
        ])]);
        let directory = tempdir().unwrap();
        let path = directory.path().join("row-below-limit.csv");

        stream_xml_to_csv_file(&xml, path.clone()).await.unwrap();

        assert!(
            fs::read_to_string(path)
                .unwrap()
                .starts_with("first,second\n")
        );
    }

    #[tokio::test]
    async fn rejects_mysql_csv_row_just_above_byte_limit() {
        let first = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES / 2);
        let second = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES - first.len() + 1);
        let xml = xml_resultset(&[xml_row(&[
            ("first", first.as_str()),
            ("second", second.as_str()),
        ])]);
        let directory = tempdir().unwrap();
        let path = directory.path().join("row-above-limit.csv");

        let error = stream_xml_to_csv_file(&xml, path).await.unwrap_err();

        assert!(
            error
                .masked_details()
                .contains(MYSQL_CSV_DECODED_ROW_LIMIT_ERROR)
        );
    }

    #[tokio::test]
    async fn rejects_mysql_csv_row_when_multiple_fields_exceed_byte_limit() {
        let first = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES / 3);
        let second = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES / 3);
        let third = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES - first.len() - second.len() + 1);
        let xml = xml_resultset(&[xml_row(&[
            ("first", first.as_str()),
            ("second", second.as_str()),
            ("third", third.as_str()),
        ])]);
        let directory = tempdir().unwrap();
        let path = directory.path().join("multi-field-row.csv");

        let error = stream_xml_to_csv_file(&xml, path).await.unwrap_err();

        assert!(
            error
                .masked_details()
                .contains(MYSQL_CSV_DECODED_ROW_LIMIT_ERROR)
        );
    }

    #[tokio::test]
    async fn does_not_publish_partial_csv_when_mysql_csv_row_exceeds_byte_limit() {
        let first = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES / 2);
        let second = "x".repeat(MYSQL_CSV_MAX_DECODED_ROW_BYTES - first.len() + 1);
        let xml = xml_resultset(&[
            xml_row(&[("status", "complete")]),
            xml_row(&[("first", first.as_str()), ("second", second.as_str())]),
        ]);
        let directory = tempdir().unwrap();
        let final_path = directory.path().join("published.csv");
        fs::write(&final_path, b"previous\n").unwrap();

        let error = export_to_path(final_path.clone(), move |temporary_path| async move {
            stream_xml_to_csv_file(&xml, temporary_path).await
        })
        .await
        .unwrap_err();

        assert!(
            error
                .masked_details()
                .contains(MYSQL_CSV_DECODED_ROW_LIMIT_ERROR)
        );
        assert_eq!(fs::read_to_string(&final_path).unwrap(), "previous\n");
        assert_eq!(directory.path().read_dir().unwrap().count(), 1);
    }

    #[tokio::test]
    async fn stops_reading_mysql_csv_field_at_byte_limit() {
        let value = "x".repeat(MYSQL_CSV_MAX_DECODED_FIELD_BYTES + 1);
        let xml =
            format!("<resultset><row><field name=\"payload\">{value}</field></row></resultset>");
        let mut source = MySqlXmlFieldLimitReader::new(std::io::Cursor::new(xml.as_bytes()));
        let mut output = Vec::new();

        let error = tokio::io::AsyncReadExt::read_to_end(&mut source, &mut output)
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), MYSQL_CSV_DECODED_FIELD_LIMIT_ERROR);
        assert!(output.len() < xml.len());
    }

    #[tokio::test]
    async fn accepts_null_fields_without_accumulating_their_xml_bytes() {
        let null_field = r#"<field name="null" xsi:nil="true"/>"#;
        let xml = format!(
            "<resultset><row>{}</row></resultset>",
            null_field.repeat(MYSQL_CSV_MAX_DECODED_FIELD_BYTES / null_field.len() + 1)
        );
        let mut source = MySqlXmlFieldLimitReader::new(std::io::Cursor::new(xml.as_bytes()));
        let mut output = Vec::new();

        tokio::io::AsyncReadExt::read_to_end(&mut source, &mut output)
            .await
            .unwrap();

        assert_eq!(output, xml.as_bytes());
    }

    #[tokio::test]
    async fn does_not_count_xml_syntax_against_mysql_csv_field_limit() {
        let escaped_value = format!("{}&amp;", "x".repeat(MYSQL_CSV_MAX_DECODED_FIELD_BYTES - 1));
        let cases = [
            format!(
                "<resultset><row><field name=\"payload\">{}</field></row></resultset>",
                "x".repeat(MYSQL_CSV_MAX_DECODED_FIELD_BYTES)
            ),
            format!(
                "<resultset><row><field name=\"payload\">{escaped_value}</field></row></resultset>"
            ),
            format!(
                "<resultset><row><field name=\"payload\"><![CDATA[{}]]></field></row></resultset>",
                "x".repeat(MYSQL_CSV_MAX_DECODED_FIELD_BYTES)
            ),
        ];

        for xml in cases {
            let mut source = MySqlXmlFieldLimitReader::new(std::io::Cursor::new(xml.as_bytes()));
            let mut output = Vec::new();
            tokio::io::AsyncReadExt::read_to_end(&mut source, &mut output)
                .await
                .unwrap();
            assert_eq!(output, xml.as_bytes());
        }
    }

    #[tokio::test]
    async fn handles_cdata_values_ending_in_brackets_before_the_next_field() {
        let cases = [
            format!("{}]", "x".repeat(MYSQL_CSV_MAX_DECODED_FIELD_BYTES - 1)),
            format!("{}]]", "x".repeat(MYSQL_CSV_MAX_DECODED_FIELD_BYTES - 2)),
        ];

        for value in cases {
            let xml = format!(
                "<resultset><row><field name=\"payload\"><![CDATA[{value}]]></field><field name=\"next\">ok</field></row></resultset>"
            );
            let mut source = MySqlXmlFieldLimitReader::new(std::io::Cursor::new(xml.as_bytes()));
            let mut output = Vec::new();
            tokio::io::AsyncReadExt::read_to_end(&mut source, &mut output)
                .await
                .unwrap();
            assert_eq!(output, xml.as_bytes());
        }
    }
}
