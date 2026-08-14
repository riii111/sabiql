use std::ffi::OsStr;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};

use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
#[cfg(not(unix))]
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncRead, BufReader, ReadBuf};
use tokio::time::timeout;
use uuid::Uuid;

use crate::adapters::csv_export::CsvFileWriter;
use crate::app::policy::sql::mysql_statement::classify_mysql_statement;
use crate::app::ports::outbound::DbOperationError;

use super::super::{
    AccessMode, MySqlDsn, MySqlOptionFile, mysql_metadata_fallback_kind, validate_mode_probe,
};
use super::error::classify_mysql_query_failure;
use super::mysql_metadata_columns;
#[cfg(not(unix))]
use super::process::read_all;
#[cfg(unix)]
use super::process::{MysqlPty, read_pty_all, write_mysql_input};
use super::process::{
    cleanup_mysql_process, configure_mysql_session, has_mysql_cli_error, read_one_mysql_resultset,
    write_mysql_statement,
};
#[cfg(unix)]
use super::xml::find_bytes;
use super::xml::{MysqlField, decode_mysql_xml_reference, parse_mysql_field, parse_mysql_xml};
use super::{MYSQL_EXPORT_TIMEOUT, MysqlProcess};

pub(crate) async fn export_mysql_csv_to_file(
    target: MySqlDsn,
    query: &str,
    path: PathBuf,
) -> Result<(), DbOperationError> {
    let option_file = MySqlOptionFile::create(&target)?;
    let mut process = MysqlProcess::spawn_with_program(OsStr::new("mysql"), &option_file.path)?;
    let result = timeout(
        MYSQL_EXPORT_TIMEOUT,
        run_mysql_export_process(&mut process, &option_file.path, query, path),
    )
    .await;
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            cleanup_mysql_process(&mut process).await;
            Err(error)
        }
        Err(_) => {
            cleanup_mysql_process(&mut process).await;
            Err(DbOperationError::Timeout(
                "mysql query exceeded the execution timeout".to_string(),
            ))
        }
    }
}

pub(crate) async fn run_mysql_export_process(
    process: &mut MysqlProcess,
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
        let columns = mysql_metadata_columns(process, option_file, query, fallback_kind).await?;
        csv_writer.write_record(columns.iter()).await?;
    }

    #[cfg(not(unix))]
    process
        .stdin
        .shutdown()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;

    #[cfg(unix)]
    let tail = {
        write_mysql_input(process, b"\x04").await?;
        read_pty_all(&mut process.pty)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?
    };

    #[cfg(not(unix))]
    let (stdout, stderr) =
        tokio::join!(read_all(&mut process.stdout), read_all(&mut process.stderr));
    #[cfg(not(unix))]
    let _stdout = stdout.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    #[cfg(not(unix))]
    let stderr = stderr.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;

    let status = process
        .child
        .wait()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    #[cfg(unix)]
    let error_bytes = tail.as_slice();
    #[cfg(not(unix))]
    let error_bytes = stderr.as_slice();
    if !status.success() {
        return Err(classify_mysql_query_failure(error_bytes));
    }

    csv_writer.finish().await
}

pub(crate) async fn stream_mysql_resultset_to_csv(
    process: &mut MysqlProcess,
    csv_writer: &mut CsvFileWriter,
) -> Result<Option<Vec<String>>, DbOperationError> {
    #[cfg(unix)]
    {
        let source = MysqlExportPtySource {
            pty: &mut process.pty,
            error_output: Vec::new(),
            pending: Vec::new(),
            started: false,
        };
        let mut reader = Reader::from_reader(BufReader::new(source));
        reader.config_mut().trim_text(false);
        let result = stream_mysql_xml_to_csv(&mut reader, csv_writer).await;
        let buffered = reader.into_inner();
        let unread = buffered.buffer().to_vec();
        let source = buffered.into_inner();
        source.pty.pending.extend(unread);
        source.pty.pending.extend(source.pending);
        if !source.error_output.is_empty() {
            return Err(classify_mysql_query_failure(&source.error_output));
        }
        result
    }

    #[cfg(not(unix))]
    {
        let source = MysqlExportPipeSource {
            stdout: &mut process.stdout,
            stderr: &mut process.stderr,
            pending: &mut process.pending,
            error_output: Vec::new(),
            stderr_buffer: [0; 4096],
            stderr_closed: false,
            stdout_closed: false,
        };
        let mut reader = Reader::from_reader(BufReader::new(source));
        reader.config_mut().trim_text(false);
        let result = stream_mysql_xml_to_csv(&mut reader, csv_writer).await;
        let buffered = reader.into_inner();
        let unread = buffered.buffer().to_vec();
        let source = buffered.into_inner();
        source.pending.extend(unread);
        if has_mysql_cli_error(&source.error_output) {
            return Err(classify_mysql_query_failure(&source.error_output));
        }
        return result;
    }
}

pub(crate) async fn stream_mysql_xml_to_csv<R>(
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
    let mut current_field: Option<MysqlField> = None;
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
                    field.value.push_str(&text);
                } else if !text.chars().all(char::is_whitespace) {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected text in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::GeneralRef(reference) => {
                let text = decode_mysql_xml_reference(&reference)?;
                if let Some(field) = current_field.as_mut() {
                    field.value.push_str(&text);
                } else if !text.chars().all(char::is_whitespace) {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected text in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::CData(data) => {
                if let Some(field) = current_field.as_mut() {
                    field
                        .value
                        .push_str(std::str::from_utf8(data.as_ref()).map_err(|error| {
                            DbOperationError::QueryFailed(format!(
                                "invalid MySQL XML text: {error}"
                            ))
                        })?);
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

#[cfg(unix)]
struct MysqlExportPtySource<'a> {
    pty: &'a mut MysqlPty,
    error_output: Vec<u8>,
    pending: Vec<u8>,
    started: bool,
}

#[cfg(unix)]
impl MysqlExportPtySource<'_> {
    fn capture_error(&mut self, bytes: &[u8]) {
        if self.error_output.is_empty() && has_mysql_cli_error(bytes) {
            self.error_output
                .extend_from_slice(&bytes[..bytes.len().min(32 * 1024)]);
        }
    }

    fn discard_before_resultset(&mut self) {
        const RESULTSET_START: &[u8] = b"<resultset";
        if let Some(start) = find_bytes(&self.pending, RESULTSET_START) {
            self.pending.drain(..start);
            self.started = true;
        } else {
            let keep = RESULTSET_START.len().saturating_sub(1);
            let discard = self.pending.len().saturating_sub(keep);
            self.pending.drain(..discard);
        }
    }
}

#[cfg(unix)]
impl AsyncRead for MysqlExportPtySource<'_> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if !this.started {
                if !this.pty.pending.is_empty() {
                    let bytes = std::mem::take(&mut this.pty.pending);
                    this.capture_error(&bytes);
                    this.pending.extend_from_slice(&bytes);
                }
                if !this.pending.is_empty() {
                    this.discard_before_resultset();
                    if this.started {
                        continue;
                    }
                }

                let mut chunk = [0; 4096];
                let mut read_buffer = ReadBuf::new(&mut chunk);
                match Pin::new(&mut this.pty.output).poll_read(cx, &mut read_buffer) {
                    Poll::Ready(Ok(())) => {
                        let bytes = read_buffer.filled().to_vec();
                        if bytes.is_empty() {
                            return Poll::Ready(Ok(()));
                        }
                        this.capture_error(&bytes);
                        this.pending.extend_from_slice(&bytes);
                    }
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => return Poll::Pending,
                }
                continue;
            }

            if !this.pending.is_empty() {
                let count = buffer.remaining().min(this.pending.len());
                let bytes = this.pending.drain(..count).collect::<Vec<_>>();
                buffer.put_slice(&bytes);
                return Poll::Ready(Ok(()));
            }

            {
                let filled_before = buffer.filled().len();
                let result = Pin::new(&mut this.pty.output).poll_read(cx, buffer);
                if matches!(&result, Poll::Ready(Ok(()))) {
                    this.capture_error(&buffer.filled()[filled_before..]);
                }
                return result;
            }
        }
    }
}

#[cfg(not(unix))]
struct MysqlExportPipeSource<'a, O, E> {
    stdout: &'a mut O,
    stderr: &'a mut E,
    pending: &'a mut Vec<u8>,
    error_output: Vec<u8>,
    stderr_buffer: [u8; 4096],
    stderr_closed: bool,
    stdout_closed: bool,
}

#[cfg(not(unix))]
impl<O, E> MysqlExportPipeSource<'_, O, E>
where
    O: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    fn capture_error(&mut self, bytes: &[u8]) {
        let remaining = (32usize * 1024).saturating_sub(self.error_output.len());
        if remaining > 0 {
            self.error_output
                .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        }
    }

    fn poll_stderr(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        if self.stderr_closed {
            return Poll::Ready(Ok(0));
        }
        let result = {
            let mut read_buffer = ReadBuf::new(&mut self.stderr_buffer);
            match Pin::new(&mut *self.stderr).poll_read(cx, &mut read_buffer) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buffer.filled().to_vec())),
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Pending => Poll::Pending,
            }
        };
        match result {
            Poll::Ready(Ok(bytes)) => {
                if bytes.is_empty() {
                    self.stderr_closed = true;
                } else {
                    self.capture_error(&bytes);
                }
                Poll::Ready(Ok(bytes.len()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(not(unix))]
impl<O, E> AsyncRead for MysqlExportPipeSource<'_, O, E>
where
    O: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let stderr_count = match this.poll_stderr(cx) {
            Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
            Poll::Pending => 0,
            Poll::Ready(Ok(count)) => count,
        };
        if !this.pending.is_empty() {
            let count = buffer.remaining().min(this.pending.len());
            let bytes = this.pending.drain(..count).collect::<Vec<_>>();
            buffer.put_slice(&bytes);
            return Poll::Ready(Ok(()));
        }

        if this.stdout_closed {
            if this.stderr_closed {
                return Poll::Ready(Ok(()));
            }
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        let filled_before = buffer.filled().len();
        let result = Pin::new(&mut *this.stdout).poll_read(cx, buffer);
        if let Poll::Ready(Ok(())) = &result {
            let count = buffer.filled().len() - filled_before;
            if count == 0 {
                this.stdout_closed = true;
                if !this.stderr_closed {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
        } else if matches!(&result, Poll::Pending) && stderr_count > 0 {
            cx.waker().wake_by_ref();
        }
        result
    }
}

#[cfg(all(test, not(unix)))]
mod export_pipe_tests {
    use super::*;

    #[tokio::test]
    async fn consumes_stderr_while_streaming_stdout() {
        let (mut stdout_writer, mut stdout_reader) = tokio::io::duplex(64);
        let (mut stderr_writer, mut stderr_reader) = tokio::io::duplex(64);
        let stdout = b"<resultset><row><field name=\"value\">ok</field></row></resultset>".to_vec();
        let stderr = format!(
            "ERROR 1146 (42S02): Table 'app.missing' doesn't exist\n{}",
            "warning\n".repeat(16 * 1024)
        )
        .into_bytes();

        let stdout_task = tokio::spawn(async move {
            stdout_writer.write_all(&stdout).await.unwrap();
        });
        let stderr_task = tokio::spawn(async move {
            stderr_writer.write_all(&stderr).await.unwrap();
        });

        let mut source = MysqlExportPipeSource {
            stdout: &mut stdout_reader,
            stderr: &mut stderr_reader,
            pending: &mut Vec::new(),
            error_output: Vec::new(),
            stderr_buffer: [0; 4096],
            stderr_closed: false,
            stdout_closed: false,
        };
        let mut output = Vec::new();
        source.read_to_end(&mut output).await.unwrap();
        stdout_task.await.unwrap();
        stderr_task.await.unwrap();

        assert!(output.starts_with(b"<resultset>"));
        assert!(has_mysql_cli_error(&source.error_output));
        assert!(matches!(
            classify_mysql_query_failure(&source.error_output),
            DbOperationError::ObjectMissing(_)
        ));
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use std::fs;

    use super::super::mysql_query_args;
    use crate::adapters::mysql::{
        MYSQL_SQL_MODE_UNSUPPORTED_MARKER, MysqlResultSet, validate_mysql_export_query,
    };
    use crate::domain::QueryValue;
    use sabiql_app::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn parses_mysql_xml_without_collapsing_value_boundaries() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<row>
  <field name="null" xsi:nil="true"/>
  <field name="empty"></field>
  <field name="text-null">NULL</field>
  <field name="special">tab&#9;line&#10;slash\unicode 日本語</field>
  <field name="json">{"a":[1,true]}</field>
  <field name="binary-looking">0x41</field>
</row>
</resultset>"#;

        let result = parse_mysql_xml(xml.as_bytes()).unwrap();

        assert_eq!(
            result.columns,
            vec![
                "null",
                "empty",
                "text-null",
                "special",
                "json",
                "binary-looking"
            ]
        );
        assert_eq!(
            result.values,
            vec![vec![
                QueryValue::Null,
                QueryValue::Text(String::new()),
                QueryValue::Text("NULL".to_string()),
                QueryValue::Text("tab\tline\nslash\\unicode 日本語".to_string()),
                QueryValue::Text("{\"a\":[1,true]}".to_string()),
                QueryValue::Text("0x41".to_string()),
            ]]
        );
    }

    #[test]
    fn parses_numeric_and_binary_values_as_text() {
        let xml = br#"<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row>
<field name="integer">18446744073709551615</field>
<field name="decimal">12345678901234567890.123456789</field>
<field name="float">1.25e+100</field>
<field name="binary">0x00FF10</field>
</row></resultset>"#;

        let result = parse_mysql_xml(xml).unwrap();
        assert!(
            result.values[0]
                .iter()
                .all(|value| matches!(value, QueryValue::Text(_)))
        );
        assert_eq!(result.values[0][0].as_str(), Some("18446744073709551615"));
        assert_eq!(result.values[0][3].as_str(), Some("0x00FF10"));
    }

    #[test]
    fn rejects_multiple_resultsets_instead_of_guessing_the_last_one() {
        let xml = br#"<resultset><row><field name="value">1</field></row></resultset>
<resultset><row><field name="value">2</field></row></resultset>"#;

        assert!(matches!(
            parse_mysql_xml(xml),
            Err(DbOperationError::QueryFailed(details))
                if details.contains("more than one resultset")
        ));
    }

    #[test]
    fn accepts_xml_declaration_after_probe_separator_and_empty_resultsets() {
        let xml = br#"
<?xml version="1.0" encoding="utf-8"?>
<resultset></resultset>
"#;

        let result = parse_mysql_xml(xml).unwrap();
        assert!(result.columns.is_empty());
        assert!(result.values.is_empty());
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
        let mut reader = Reader::from_reader(BufReader::new(output));
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

    #[test]
    fn csv_export_accepts_one_read_only_result_query() {
        assert!(validate_mysql_export_query("SELECT 1", Some("app")).is_ok());
        for query in ["TABLE users", "SHOW TABLES", "DESCRIBE users"] {
            assert!(
                validate_mysql_export_query(query, Some("app")).is_ok(),
                "{query}"
            );
        }
        assert!(matches!(
            validate_mysql_export_query("INSERT INTO users VALUES (1)", Some("app")),
            Err(DbOperationError::PermissionDenied(_))
        ));
        assert!(matches!(
            validate_mysql_export_query("SELECT 1; SELECT 2", Some("app")),
            Err(DbOperationError::UnsupportedOperation(details))
                if details.contains("single read-only result")
        ));
    }

    #[test]
    fn mode_probe_requires_marker_and_allowed_mode_before_user_sql() {
        let probe = MysqlResultSet {
            columns: vec![
                "__sabiql_probe".to_string(),
                "__sabiql_sql_mode".to_string(),
            ],
            values: vec![vec![
                QueryValue::Text("marker".to_string()),
                QueryValue::Text("STRICT_TRANS_TABLES".to_string()),
            ]],
        };
        assert!(validate_mode_probe(&probe, "marker").is_ok());

        let mut unsupported = probe;
        unsupported.values[0][1] = QueryValue::Text("ANSI_QUOTES".to_string());
        assert!(matches!(
            validate_mode_probe(&unsupported, "marker"),
            Err(DbOperationError::UnsupportedOperation(details))
                if details.contains(MYSQL_SQL_MODE_UNSUPPORTED_MARKER)
        ));
    }

    #[test]
    fn arguments_keep_credentials_out_of_argv() {
        let args = mysql_query_args(std::path::Path::new("/tmp/sabiql-mysql.cnf"));

        assert_eq!(args[0], "--defaults-file=/tmp/sabiql-mysql.cnf");
        assert_eq!(args[1], "--no-login-paths");
        for expected in [
            "--xml",
            "--binary-as-hex",
            "--binary-mode",
            "--unbuffered",
            "--skip-reconnect",
            "--default-character-set=utf8mb4",
        ] {
            assert!(args.contains(&expected.to_string()), "{expected}");
        }
        assert!(args.contains(&"--batch".to_string()));
        assert!(args.iter().all(|argument| !argument.contains("password")));
    }

    #[test]
    fn classifies_mysql_query_failures_by_server_error() {
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 1045 (28000): Access denied for user 'app'@'localhost' (using password: YES)"
            ),
            DbOperationError::ConnectionFailed(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1142 (42000): command denied to user"),
            DbOperationError::PermissionDenied(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 1044 (42000): Access denied for user 'app' to database 'app'"
            ),
            DbOperationError::PermissionDenied(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1146 (42S02): Table does not exist"),
            DbOperationError::ObjectMissing(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(b"ERROR 1205 (HY000): Lock wait timeout exceeded"),
            DbOperationError::LockTimeout(_)
        ));
        assert!(matches!(
            classify_mysql_query_failure(
                b"ERROR 1452 (23000): Cannot add or update a child row: a foreign key constraint fails"
            ),
            DbOperationError::ForeignKeyViolation(_)
        ));
        let masked = classify_mysql_query_failure(b"ERROR password=secret");
        assert!(!masked.masked_details().contains("secret"));
    }

    #[test]
    fn classifies_mysql_tls_query_failures_as_connection_errors() {
        let error = classify_mysql_query_failure(
            b"ERROR 2026 (HY000): SSL connection error: error:0A000086:SSL routines::certificate verify failed",
        );

        assert_eq!(
            ConnectionErrorInfo::from_db_operation_error(&error).kind,
            ConnectionErrorKind::MySqlTlsHandshakeFailed
        );
        assert!(matches!(error, DbOperationError::ConnectionFailed(_)));
    }
}
