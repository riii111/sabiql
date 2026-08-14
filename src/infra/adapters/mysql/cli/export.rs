pub(super) async fn export_mysql_csv_to_file(
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

async fn run_mysql_export_process(
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
        return result;
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
mod query_tests {
    use sabiql_app::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};
    use tempfile::tempdir;

    use super::*;

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

#[cfg(all(test, unix))]
mod executor_tests {
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    async fn export_mysql_csv_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
        query: &str,
        path: PathBuf,
        execution_timeout: Duration,
    ) -> Result<(), DbOperationError> {
        let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
        let result = timeout(
            execution_timeout,
            run_mysql_export_process(&mut process, option_file, query, path),
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

    async fn run_mysql_adhoc_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
        query: &str,
        access_mode: AccessMode,
        execution_timeout: Duration,
    ) -> Result<MysqlResultSet, DbOperationError> {
        let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
        let result = timeout(
            execution_timeout,
            run_mysql_single_statement_process(&mut process, query, access_mode),
        )
        .await;
        match result {
            Ok(Ok(result_set)) => Ok(result_set),
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

    #[test]
    fn failure_before_a_change_keeps_original_error() {
        let error = query_failed_after_change(
            DbOperationError::ForeignKeyViolation("foreign key failed".to_string()),
            RefreshScope::None,
        );

        assert!(matches!(
            error,
            DbOperationError::ForeignKeyViolation(details) if details == "foreign key failed"
        ));
    }

    fn fake_mysql(mode: &str) -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let program = directory.path().join("mysql");
        let probe_response = match mode {
            "missing" => "exit 0".to_string(),
            "invalid" => {
                "printf '%s\\n' '<resultset><row><field name=\"wrong\">x</field></row></resultset>'"
                    .to_string()
            }
            "unsupported" => "printf '%s\\n' '<resultset><row><field name=\"__sabiql_probe\">'\"$marker\"'</field><field name=\"__sabiql_sql_mode\">ANSI_QUOTES</field></row></resultset>'".to_string(),
            "timeout" => "while :; do :; done".to_string(),
            _ => "printf '%s\\n' '<resultset><row><field name=\"__sabiql_probe\">'\"$marker\"'</field><field name=\"__sabiql_sql_mode\">STRICT_TRANS_TABLES</field></row></resultset>'".to_string(),
        };
        let user_response = if mode == "failure" {
            "printf '%s\\n' '<resultset><row><field name=\"partial\">row</field></row></resultset>'\n    printf '%s\\n' 'ERROR 1064 (42000): syntax error' >&2\n    exit 1"
        } else if mode == "no_result_failure" {
            "printf '%s\\n' 'ERROR 1054 (42S22): Unknown column missing_column' >&2\n    exit 1"
        } else {
            "printf '%s\\n' '<resultset><row><field name=\"value\">ok</field></row></resultset>'"
        };
        let session_failure = if mode == "read_only_failure" {
            "printf '%s\\n' 'ERROR 1227 (42000): access denied to set transaction read only' >&2\n      exit 1"
        } else {
            ""
        };
        let script = format!(
            r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
phase=probe
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  [ "$line" = ";" ] && continue
  if [ "$phase" = probe ]; then
    marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)'.*/\\1/")
    {probe_response}
    phase=user
  else
    case "$line" in
      "SET SESSION TRANSACTION READ ONLY")
        {session_failure}
        ;;
      *__sabiql_session_marker*)
        marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
        printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
        ;;
      *)
        {user_response}
        exit 0
        ;;
    esac
  fi
done
"#,
        );
        fs::write(&program, script).unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, log_file)
    }

    fn fake_mysql_multi() -> (TempDir, PathBuf, PathBuf) {
        fake_mysql_multi_with_mode(false, None)
    }

    fn fake_mysql_multi_with_marker_failure() -> (TempDir, PathBuf, PathBuf) {
        fake_mysql_multi_with_mode(true, None)
    }

    fn fake_mysql_multi_with_statement_failure(error: &str) -> (TempDir, PathBuf, PathBuf) {
        fake_mysql_multi_with_mode(false, Some(error))
    }

    fn fake_mysql_multi_with_mode(
        marker_failure: bool,
        statement_error: Option<&str>,
    ) -> (TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let option_file = directory.path().join("option.cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let program = directory.path().join("mysql");
        let update_response = statement_error.map_or_else(
            || {
                "printf '%s\\n' '<resultset><row><field name=\"affected\">ok</field></row></resultset>'"
                    .to_string()
            },
            |error| format!("printf '%s\\n' '{error}' >&2"),
        );
        let marker_response = if marker_failure {
            "printf '%s\\n' '<resultset><row><field name=\"wrong\">x</field></row></resultset>'"
        } else {
            "marker=$(printf '%s\\n' \"$line\" | sed \"s/.*SELECT '\\\\([^']*\\\\)' AS __sabiql_marker.*/\\\\1/\")
      rows=0
      case \"$line\" in *ROW_COUNT\\(\\)* ) rows=3 ;; esac
      printf '%s\\n' '<resultset><row><field name=\"__sabiql_marker\">'\"$marker\"'</field><field name=\"affected_rows\">'\"$rows\"'</field></row></resultset>'
      if [ \"$pending_error\" = 1 ]; then
        sleep 0.05
        printf '%s\\n' 'ERROR 1054 (42S22): Unknown column missing_column' >&2
        pending_error=0
      fi"
        };
        let script = format!(
            r#"#!/bin/sh
option=$(printf '%s\n' "$1" | sed 's/^--defaults-file=//')
log="$option.log"
printf 'process=%s\n' "$$" >> "$log"
pending_error=0
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_probe.*/\\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_probe">'"$marker"'</field><field name="__sabiql_sql_mode">STRICT_TRANS_TABLES</field></row></resultset>'
      ;;
    "SET SESSION TRANSACTION READ ONLY")
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      printf '%s\n' '<resultset><row><field name="__sabiql_session_marker">'"$marker"'</field></row></resultset>'
      ;;
      *__sabiql_marker*)
      {marker_response}
      ;;
    *missing_column*)
      pending_error=1
      ;;
    *SELECT*)
      value=one
      case "$line" in *SELECT\ 2*) value=two ;; esac
      printf '%s\n' '<resultset><row><field name="value">'"$value"'</field></row></resultset>'
      ;;
    *UPDATE*)
      {update_response}
      ;;
    *)
      printf '%s\n' '<resultset></resultset>'
      ;;
  esac
done
"#,
        );
        fs::write(&program, script).unwrap();
        let mut permissions = fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&program, permissions).unwrap();
        (directory, program, option_file)
    }

    #[tokio::test]
    async fn sends_user_sql_only_after_a_valid_mode_probe() {
        let (_directory, program, log_file) = fake_mysql("success");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 123",
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.values[0][0].as_str(), Some("ok"));
        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
        assert!(log.contains("__sabiql_probe"));
        assert!(log.contains("SELECT 123"));
        assert!(!log.contains(MYSQL_READ_ONLY_STATEMENT));
    }

    #[tokio::test]
    async fn configures_read_only_session_before_user_sql() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let statements = split_mysql_statements("SELECT 2")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "SELECT 2",
            &statements,
            AccessMode::ReadOnly,
            Duration::from_secs(5),
        )
        .await
        .unwrap_or_else(|error| {
            let log = fs::read_to_string(&log_file).unwrap_or_default();
            panic!("read-only execution failed: {error:?}; log: {log}");
        });

        assert_eq!(
            result.result_set.unwrap().values[0][0].as_str(),
            Some("two")
        );
        let log = fs::read_to_string(log_file).unwrap();
        let session_index = log
            .find(MYSQL_READ_ONLY_STATEMENT)
            .expect("read-only session statement");
        let user_index = log.find("SELECT 2").expect("user statement");
        assert!(session_index < user_index, "{log}");
        assert!(log.contains(MYSQL_SESSION_MARKER_COLUMN));
    }

    #[tokio::test]
    async fn metadata_session_reuses_one_process_for_ordered_resultsets() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let mut session =
            MysqlMetadataSession::spawn_with_program(OsStr::new(&program), &option_file)
                .expect("spawn fake mysql");

        session.probe().await.expect("mode probe");
        for query in [
            "SELECT TABLES",
            "SELECT COLUMNS",
            "SELECT INDEXES",
            "SELECT FOREIGN_KEYS",
            "SELECT TRIGGERS",
            "SHOW CREATE TABLE items",
        ] {
            session.execute(query).await.expect("metadata resultset");
        }
        session.finish().await.expect("finish fake mysql");

        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
        assert_eq!(
            log.lines()
                .filter(|line| line.starts_with("process="))
                .count(),
            1
        );
        let positions = [
            "__sabiql_probe",
            "SELECT TABLES",
            "SELECT COLUMNS",
            "SELECT INDEXES",
            "SELECT FOREIGN_KEYS",
            "SELECT TRIGGERS",
            "SHOW CREATE TABLE items",
        ]
        .into_iter()
        .map(|query| log.find(query).expect("query in transcript"))
        .collect::<Vec<_>>();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{log}");
    }

    #[tokio::test]
    async fn read_only_session_failure_never_writes_user_sql() {
        let (_directory, program, log_file) = fake_mysql("read_only_failure");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 123",
            AccessMode::ReadOnly,
            Duration::from_secs(5),
        )
        .await;

        assert!(result.is_err());
        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
        assert!(log.contains(MYSQL_READ_ONLY_STATEMENT));
        assert!(!log.contains("SELECT 123"), "{log}");
    }

    #[tokio::test]
    async fn generated_preview_and_metadata_queries_skip_read_only_session_setup() {
        for query in [
            "SELECT id FROM app.items ORDER BY id LIMIT 10 OFFSET 0",
            "SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES",
        ] {
            let (_directory, program, option_file) = fake_mysql_multi();
            let statements = split_mysql_statements(query)
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            run_mysql_adhoc_with_program_and_statements(
                OsStr::new(&program),
                &option_file,
                query,
                &statements,
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

            let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
            assert!(!log.contains(MYSQL_READ_ONLY_STATEMENT), "{query}: {log}");
        }
    }

    #[test]
    fn read_only_rejects_temporary_table_dml_before_starting_mysql() {
        let (_directory, _program, option_file) = fake_mysql_multi();
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let query = "CREATE TEMPORARY TABLE temp_items (id INT); INSERT INTO temp_items VALUES (1); DROP TEMPORARY TABLE temp_items";

        let result = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadOnly);

        assert!(matches!(
            result,
            Err(DbOperationError::PermissionDenied(details))
                if details.contains("read-only mode blocks MySQL write statements")
        ));
        assert!(!log_file.exists());
    }

    #[test]
    fn read_only_rejects_read_write_overrides_before_starting_mysql() {
        for query in [
            "SET SESSION TRANSACTION READ WRITE",
            "START TRANSACTION READ WRITE",
        ] {
            let (_directory, _program, option_file) = fake_mysql_multi();
            let log_file = PathBuf::from(format!("{}.log", option_file.display()));

            let result = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadOnly);

            assert!(matches!(
                result,
                Err(DbOperationError::UnsupportedOperation(_))
            ));
            assert!(!log_file.exists(), "{query}");
        }
    }

    #[test]
    fn rejects_empty_metadata_fallback_after_temporary_table_creation() {
        for query in [
            "CREATE TEMPORARY TABLE temp_items (id INT); DESCRIBE temp_items 'missing'; DROP TEMPORARY TABLE temp_items",
            "CREATE TEMPORARY TABLE temp_items (id INT); SHOW COLUMNS FROM temp_items LIKE 'missing'; DROP TEMPORARY TABLE temp_items",
        ] {
            let statements = validate_mysql_multi_query(query, Some("app"), AccessMode::ReadWrite)
                .expect("query should be classified before the session-state check");

            assert!(mysql_metadata_fallback_has_unsupported_session_state(
                &statements
            ));
        }

        let statements = validate_mysql_multi_query(
            "SHOW COLUMNS FROM items",
            Some("app"),
            AccessMode::ReadWrite,
        )
        .expect("single SHOW should be classified");
        assert!(!mysql_metadata_fallback_has_unsupported_session_state(
            &statements
        ));
    }

    #[tokio::test]
    async fn exports_mysql_xml_rows_through_the_shared_csv_writer() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let path = option_file.with_file_name("export.csv");

        export_mysql_csv_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 1",
            path.clone(),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "value\none\n");
    }

    #[tokio::test]
    async fn export_configures_read_only_session_before_user_sql() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let path = option_file.with_file_name("export.csv");

        export_mysql_csv_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 1",
            path,
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        let log = fs::read_to_string(log_file).unwrap();
        let session_index = log
            .find(MYSQL_READ_ONLY_STATEMENT)
            .expect("read-only session statement");
        let user_index = log.find("SELECT 1").expect("user statement");
        assert!(session_index < user_index, "{log}");
        assert!(log.contains(MYSQL_SESSION_MARKER_COLUMN));
    }

    #[tokio::test]
    async fn export_read_only_session_failure_never_writes_user_sql_or_partial_file() {
        let (_directory, program, option_file) = fake_mysql("read_only_failure");
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let output_directory = tempfile::tempdir().unwrap();
        let final_path = output_directory.path().join("export.csv");

        let result = export_to_path(final_path.clone(), |path| {
            export_mysql_csv_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                path,
                Duration::from_secs(5),
            )
        })
        .await;

        assert!(result.is_err());
        let log = fs::read_to_string(log_file).unwrap();
        assert!(log.contains(MYSQL_READ_ONLY_STATEMENT));
        assert!(!log.contains("SELECT 123"), "{log}");
        assert!(!final_path.exists());
        assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
    }

    #[tokio::test]
    async fn export_failure_removes_the_partial_file() {
        let (_directory, program, option_file) = fake_mysql("failure");
        let output_directory = tempfile::tempdir().unwrap();
        let final_path = output_directory.path().join("export.csv");

        let result = export_to_path(final_path.clone(), |path| {
            export_mysql_csv_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 1",
                path,
                Duration::from_secs(5),
            )
        })
        .await;

        assert!(result.is_err());
        assert!(!final_path.exists());
        assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
    }

    #[tokio::test]
    async fn export_timeout_kills_the_process_and_removes_the_partial_file() {
        let (_directory, program, option_file) = fake_mysql("timeout");
        let output_directory = tempfile::tempdir().unwrap();
        let final_path = output_directory.path().join("export.csv");

        let result = export_to_path(final_path.clone(), |path| {
            export_mysql_csv_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 1",
                path,
                Duration::from_millis(50),
            )
        })
        .await;

        assert!(matches!(result, Err(DbOperationError::Timeout(_))));
        assert!(!final_path.exists());
        assert_eq!(output_directory.path().read_dir().unwrap().count(), 0);
    }

    #[test]
    fn frames_one_xml_resultset_and_preserves_following_output() {
        let mut buffer = b"    -> <?xml version=\"1.0\"?>\n<resultset></resultset>\r\n    -> <?xml version=\"1.0\"?>\n<resultset>"
            .to_vec();
        let mut scanner = MysqlResultsetFrameScanner::default();

        assert_eq!(
            scanner.take(&mut buffer),
            Some(b"<resultset></resultset>".to_vec())
        );
        assert_eq!(
            scanner.take(&mut buffer),
            None,
            "an incomplete following frame must remain buffered"
        );
        assert!(buffer.starts_with(b"\r\n    -> <?xml"));
    }

    #[test]
    fn frames_resultset_after_mysql_cli_text() {
        let mut buffer =
            b"SELECT 1;\n<?xml version=\"1.0\"?>\nquery text\n<resultset></resultset>".to_vec();
        let mut scanner = MysqlResultsetFrameScanner::default();

        assert_eq!(
            scanner.take(&mut buffer),
            Some(b"<resultset></resultset>".to_vec())
        );
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn probe_failure_never_writes_user_sql() {
        for mode in ["unsupported", "invalid", "missing"] {
            let (_directory, program, log_file) = fake_mysql(mode);
            let option_file = log_file.with_extension("cnf");
            fs::write(&option_file, "[client]\n").unwrap();
            let result = run_mysql_adhoc_with_program(
                OsStr::new(&program),
                &option_file,
                "SELECT 123",
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await;
            assert!(result.is_err(), "{mode}");
            let log =
                fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
            assert!(!log.contains("SELECT 123"), "{mode}: {log}");
        }
    }

    #[tokio::test]
    async fn probe_timeout_kills_the_process_and_discards_output() {
        let (_directory, program, log_file) = fake_mysql("timeout");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 123",
            AccessMode::ReadWrite,
            Duration::from_millis(50),
        )
        .await;

        assert!(matches!(result, Err(DbOperationError::Timeout(_))));
        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap_or_default();
        assert!(!log.contains("SELECT 123"));
    }

    #[tokio::test]
    async fn nonzero_cli_exit_discards_any_collected_stdout() {
        let (_directory, program, log_file) = fake_mysql("failure");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 123",
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
    }

    #[tokio::test]
    async fn classifies_cli_error_when_no_resultset_is_emitted() {
        let (_directory, program, log_file) = fake_mysql("no_result_failure");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 123",
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(
            result,
            Err(DbOperationError::QueryFailed(details))
                if details.contains("missing_column")
        ));
    }

    #[tokio::test]
    async fn executes_each_statement_and_returns_the_last_user_result() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let log_file = PathBuf::from(format!("{}.log", option_file.display()));
        let statements = split_mysql_statements("UPDATE items SET value = 1; SELECT 2")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "UPDATE items SET value = 1; SELECT 2",
            &statements,
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await
        .unwrap_or_else(|error| panic!("multi execution failed: {error:?}"));

        assert_eq!(
            result.result_set,
            Some(MysqlResultSet {
                columns: vec!["value".to_string()],
                values: vec![vec![QueryValue::Text("two".to_string())]],
            })
        );
        assert_eq!(result.command_tag, Some(CommandTag::Update(3)));
        assert_eq!(result.refresh_scope, RefreshScope::Data);
        let log = fs::read_to_string(log_file).unwrap();
        assert!(log.contains("UPDATE items SET value = 1"));
        assert!(log.matches("__sabiql_marker").count() >= 2);
    }

    #[tokio::test]
    async fn marker_failure_after_a_change_refreshes_the_current_scope() {
        let (_directory, program, option_file) = fake_mysql_multi_with_marker_failure();
        let statements = split_mysql_statements("UPDATE items SET value = 1")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "UPDATE items SET value = 1",
            &statements,
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(
            result,
            Err(DbOperationError::QueryFailedAfterChange {
                source,
                refresh_scope: RefreshScope::Data,
                ..
            }) if matches!(&*source, DbOperationError::QueryFailed(_))
        ));
    }

    #[tokio::test]
    async fn first_change_statement_failure_keeps_the_classified_error_unwrapped() {
        for (details, summary) in [
            (
                "ERROR 1142 (42000): command denied to user",
                "Permission denied",
            ),
            (
                "ERROR 1062 (23000): Duplicate entry duplicate_value for key PRIMARY",
                "Unique constraint violation",
            ),
            (
                "ERROR 1452 (23000): Cannot add or update a child row: a foreign key constraint fails",
                "Foreign key constraint violation",
            ),
            (
                "ERROR 1205 (HY000): Lock wait timeout exceeded",
                "Operation blocked by lock or timeout",
            ),
        ] {
            let (_directory, program, option_file) =
                fake_mysql_multi_with_statement_failure(details);
            let statements = split_mysql_statements("UPDATE items SET value = 1")
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

            let result = run_mysql_adhoc_with_program_and_statements(
                OsStr::new(&program),
                &option_file,
                "UPDATE items SET value = 1",
                &statements,
                AccessMode::ReadWrite,
                Duration::from_secs(5),
            )
            .await;
            let Err(error) = result else {
                panic!("expected the fake MySQL statement to fail");
            };

            assert_eq!(error.summary(), summary);
            assert!(!matches!(
                error,
                DbOperationError::QueryFailedAfterChange { .. }
            ));
        }
    }

    #[tokio::test]
    async fn marks_a_later_failure_after_a_confirmed_change_for_refresh() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let statements =
            split_mysql_statements("UPDATE items SET value = 1; SELECT missing_column FROM items")
                .unwrap()
                .into_iter()
                .map(|sql| classify_mysql_statement(&sql).unwrap())
                .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "UPDATE items SET value = 1; SELECT missing_column FROM items",
            &statements,
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(
            result,
            Err(DbOperationError::QueryFailedAfterChange {
                source,
                refresh_scope: RefreshScope::Data,
                ..
            }) if matches!(&*source, DbOperationError::QueryFailed(_))
        ));
    }

    #[tokio::test]
    async fn rejects_error_reported_after_row_count_marker() {
        let (_directory, program, option_file) = fake_mysql_multi();
        let statements = split_mysql_statements("SELECT missing_column FROM items")
            .unwrap()
            .into_iter()
            .map(|sql| classify_mysql_statement(&sql).unwrap())
            .collect::<Vec<_>>();

        let result = run_mysql_adhoc_with_program_and_statements(
            OsStr::new(&program),
            &option_file,
            "SELECT missing_column FROM items",
            &statements,
            AccessMode::ReadWrite,
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(
            result,
            Err(DbOperationError::QueryFailed(details))
                if details.contains("missing_column")
        ));
    }

    #[test]
    fn transaction_rollback_removes_pending_data_tag() {
        let events = vec![
            MysqlCommandEvent {
                kind: MysqlStatementKind::Begin,
                target: None,
                tag: CommandTag::Begin,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Update { has_where: true },
                target: Some("items".to_string()),
                tag: CommandTag::Update(1),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Rollback,
                target: None,
                tag: CommandTag::Rollback,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Select,
                target: None,
                tag: CommandTag::Select(1),
            },
        ];

        assert_eq!(
            aggregate_mysql_command_tag(&events),
            Some(CommandTag::Select(1))
        );
    }

    #[test]
    fn ddl_implicit_commit_keeps_prior_data_change() {
        let events = vec![
            MysqlCommandEvent {
                kind: MysqlStatementKind::Begin,
                target: None,
                tag: CommandTag::Begin,
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Insert,
                target: Some("items".to_string()),
                tag: CommandTag::Insert(1),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::CreateTable { temporary: false },
                target: Some("created".to_string()),
                tag: CommandTag::Create("TABLE".to_string()),
            },
            MysqlCommandEvent {
                kind: MysqlStatementKind::Rollback,
                target: None,
                tag: CommandTag::Rollback,
            },
        ];

        assert_eq!(
            aggregate_mysql_command_tag(&events),
            Some(CommandTag::Create("TABLE".to_string()))
        );
    }
}
