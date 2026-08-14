use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
#[cfg(not(unix))]
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncRead, BufReader};
use tokio::time::timeout;
use uuid::Uuid;

use crate::adapters::csv_export::CsvFileWriter;
use crate::app::policy::sql::mysql_statement::classify_mysql_statement;
use crate::app::ports::outbound::{AccessMode, DbOperationError};

use super::super::{dsn::MySqlDsn, option_file::MySqlOptionFile};
#[cfg(not(unix))]
use super::error::has_mysql_cli_error;
use super::error::{classify_mysql_query_failure, validate_mode_probe};
#[cfg(not(unix))]
use super::pipe::{MysqlExportPipeSource, read_all};
use super::policy::mysql_metadata_fallback_kind;
#[cfg(unix)]
use super::process::write_mysql_input;
use super::process::{
    MYSQL_QUERY_TIMEOUT, MysqlProcess, cleanup_mysql_process, configure_mysql_session,
    mysql_metadata_columns, read_one_mysql_resultset, write_mysql_statement,
};
#[cfg(unix)]
use super::pty::{MysqlExportPtySource, read_pty_all};
use super::xml::{MysqlField, decode_mysql_xml_reference, parse_mysql_field, parse_mysql_xml};

const MYSQL_EXPORT_TIMEOUT: Duration = Duration::from_secs(MYSQL_QUERY_TIMEOUT.as_secs() * 10);

pub(in crate::adapters::mysql) async fn export_mysql_csv_to_file(
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

pub(super) async fn run_mysql_export_process(
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

async fn stream_mysql_resultset_to_csv(
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

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
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
}
