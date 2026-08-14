pub(super) struct MysqlProcess {
    child: Child,
    #[cfg(unix)]
    pty: MysqlPty,
    #[cfg(not(unix))]
    stdin: ChildStdin,
    #[cfg(not(unix))]
    stdout: ChildStdout,
    #[cfg(not(unix))]
    stderr: ChildStderr,
    #[cfg(not(unix))]
    pending: Vec<u8>,
    #[cfg(not(unix))]
    pending_stderr: Vec<u8>,
    #[cfg(not(unix))]
    frame_scanner: MysqlResultsetFrameScanner,
}

impl MysqlProcess {
    pub(super) fn spawn_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        #[cfg(unix)]
        {
            Self::spawn_with_pty(program, option_file)
        }

        #[cfg(not(unix))]
        {
            let mut command = Command::new(program);
            command
                .args(mysql_query_args(option_file))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .env_remove("MYSQL_PWD")
                .env_remove("MYSQL_PASSWORD")
                .kill_on_drop(true);
            let mut child = command.spawn().map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    DbOperationError::CommandNotFound {
                        command: DatabaseCli::MySql,
                        details: error.to_string(),
                    }
                } else {
                    DbOperationError::ConnectionFailed(error.to_string())
                }
            })?;
            let stdin = child.stdin.take().ok_or_else(|| {
                DbOperationError::QueryFailed("mysql stdin was not piped".to_string())
            })?;
            let stdout = child.stdout.take().ok_or_else(|| {
                DbOperationError::QueryFailed("mysql stdout was not piped".to_string())
            })?;
            let stderr = child.stderr.take().ok_or_else(|| {
                DbOperationError::QueryFailed("mysql stderr was not piped".to_string())
            })?;
            return Ok(Self {
                child,
                stdin,
                stdout,
                stderr,
                pending: Vec::new(),
                pending_stderr: Vec::new(),
                frame_scanner: MysqlResultsetFrameScanner::default(),
            });
        }
    }

    #[cfg(unix)]
    fn spawn_with_pty(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        let (master, slave) = create_mysql_pty().map_err(|error| {
            DbOperationError::ConnectionFailed(format!("Unable to create MySQL PTY: {error}"))
        })?;
        let mut command = Command::new(program);
        command
            .args(mysql_query_args(option_file))
            .stdin(Stdio::from(slave.try_clone().map_err(|error| {
                DbOperationError::ConnectionFailed(error.to_string())
            })?))
            .stdout(Stdio::from(slave.try_clone().map_err(|error| {
                DbOperationError::ConnectionFailed(error.to_string())
            })?))
            .stderr(Stdio::from(slave))
            .env_remove("MYSQL_PWD")
            .env_remove("MYSQL_PASSWORD")
            .kill_on_drop(true);
        let child = command.spawn().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                DbOperationError::CommandNotFound {
                    command: DatabaseCli::MySql,
                    details: error.to_string(),
                }
            } else {
                DbOperationError::ConnectionFailed(error.to_string())
            }
        })?;
        let output = TokioFile::from_std(
            master
                .try_clone()
                .map_err(|error| DbOperationError::ConnectionFailed(error.to_string()))?,
        );
        let input = TokioFile::from_std(master);
        Ok(Self {
            child,
            pty: MysqlPty {
                input,
                output,
                pending: Vec::new(),
                frame_scanner: MysqlResultsetFrameScanner::default(),
            },
        })
    }
}

pub(super) struct MysqlMetadataSession {
    process: MysqlProcess,
}

impl MysqlMetadataSession {
    pub(super) fn spawn_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        Ok(Self {
            process: MysqlProcess::spawn_with_program(program, option_file)?,
        })
    }

    pub(super) async fn probe(&mut self) -> Result<(), DbOperationError> {
        let marker = Uuid::new_v4().simple().to_string();
        let query =
            format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
        let result = self.execute(&query).await?;
        validate_mode_probe(&result, &marker)
    }

    pub(super) async fn execute(&mut self, query: &str) -> Result<MysqlResultSet, DbOperationError> {
        write_mysql_statement(&mut self.process, query).await?;
        let xml = read_one_mysql_resultset(&mut self.process).await?;
        parse_mysql_xml(&xml)
    }

    pub(super) async fn finish(&mut self) -> Result<(), DbOperationError> {
        #[cfg(not(unix))]
        self.process
            .stdin
            .shutdown()
            .await
            .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;

        #[cfg(unix)]
        let tail = {
            write_mysql_input(&mut self.process, b"\x04").await?;
            read_pty_all(&mut self.process.pty)
                .await
                .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?
        };

        #[cfg(not(unix))]
        let (_stdout, stderr) = tokio::join!(
            read_all(&mut self.process.stdout),
            read_all(&mut self.process.stderr)
        );
        #[cfg(not(unix))]
        let stderr = stderr.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;

        let status = self
            .process
            .child
            .wait()
            .await
            .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
        #[cfg(unix)]
        let error_bytes = tail.as_slice();
        #[cfg(not(unix))]
        let error_bytes = stderr.as_slice();
        if has_mysql_cli_error(error_bytes) {
            return Err(classify_mysql_query_failure(error_bytes));
        }
        if !status.success() {
            return Err(classify_mysql_query_failure(error_bytes));
        }
        Ok(())
    }

    pub(super) async fn cleanup(&mut self) {
        cleanup_mysql_process(&mut self.process).await;
    }
}

pub(super) async fn run_mysql_adhoc(
    option_file: &std::path::Path,
    query: &str,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
) -> Result<MysqlExecutionResult, DbOperationError> {
    run_mysql_adhoc_with_program_and_statements(
        OsStr::new("mysql"),
        option_file,
        query,
        statements,
        access_mode,
        MYSQL_QUERY_TIMEOUT,
    )
    .await
}

pub(super) async fn run_mysql_single_statement(
    option_file: &std::path::Path,
    query: &str,
    access_mode: AccessMode,
) -> Result<MysqlResultSet, DbOperationError> {
    let mut process = MysqlProcess::spawn_with_program(OsStr::new("mysql"), option_file)?;
    let result = timeout(
        MYSQL_QUERY_TIMEOUT,
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

async fn run_mysql_single_statement_process(
    process: &mut MysqlProcess,
    query: &str,
    access_mode: AccessMode,
) -> Result<MysqlResultSet, DbOperationError> {
    let marker = Uuid::new_v4().simple().to_string();
    let probe_query =
        format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
    write_mysql_statement(process, &probe_query).await?;
    let probe_xml = read_one_mysql_resultset(process).await?;
    let probe = parse_mysql_xml(&probe_xml)?;
    validate_mode_probe(&probe, &marker)?;
    configure_mysql_session(process, access_mode).await?;

    write_mysql_statement(process, query).await?;

    #[cfg(not(unix))]
    process
        .stdin
        .shutdown()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;

    #[cfg(unix)]
    let (stdout, tail) = {
        let stdout = read_one_mysql_resultset(process).await?;
        write_mysql_input(process, b"\x04").await?;
        let tail = read_pty_all(&mut process.pty)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        (stdout, tail)
    };

    #[cfg(not(unix))]
    let (stdout, stderr) =
        tokio::join!(read_all(&mut process.stdout), read_all(&mut process.stderr));
    #[cfg(not(unix))]
    let stdout = stdout.map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
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
    parse_mysql_xml(&stdout)
}

async fn run_mysql_adhoc_with_program_and_statements(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
    execution_timeout: Duration,
) -> Result<MysqlExecutionResult, DbOperationError> {
    if mysql_metadata_fallback_has_unsupported_session_state(statements) {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL empty SHOW/DESCRIBE metadata fallback cannot preserve temporary-table session state"
                .to_string(),
        ));
    }
    let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
    let result = timeout(
        execution_timeout,
        run_mysql_adhoc_process(&mut process, option_file, query, statements, access_mode),
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

async fn run_mysql_adhoc_process(
    process: &mut MysqlProcess,
    option_file: &std::path::Path,
    _query: &str,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
) -> Result<MysqlExecutionResult, DbOperationError> {
    let probe_marker = Uuid::new_v4().simple().to_string();
    let probe_query = format!(
        "SELECT '{probe_marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode"
    );
    write_mysql_statement(process, &probe_query).await?;
    let probe_xml = read_one_mysql_resultset(process).await?;
    let probe = parse_mysql_xml(&probe_xml)?;
    validate_mode_probe(&probe, &probe_marker)?;
    configure_mysql_session(process, access_mode).await?;

    let mut last_result_set = None;
    let mut command_tags = Vec::with_capacity(statements.len());
    let mut refresh_scope = RefreshScope::None;
    let mut scope_before_statement = RefreshScope::None;

    for statement in statements {
        scope_before_statement = refresh_scope;
        let marker = Uuid::new_v4().simple().to_string();
        let statement_scope = mysql_refresh_scope(&statement.kind);
        let possible_refresh_scope = refresh_scope.merge(statement_scope);
        if let Err(error) = write_mysql_statement(process, &statement.sql).await {
            return Err(query_failed_after_change(error, refresh_scope));
        }
        let marker_query =
            format!("SELECT '{marker}' AS __sabiql_marker, ROW_COUNT() AS affected_rows");
        if let Err(error) = write_mysql_statement(process, &marker_query).await {
            return Err(query_failed_after_change(error, possible_refresh_scope));
        }
        let first_xml = match read_one_mysql_resultset(process).await {
            Ok(xml) => xml,
            Err(error) => {
                return Err(query_failed_after_mysql_statement(
                    error,
                    refresh_scope,
                    possible_refresh_scope,
                ));
            }
        };
        let first_result = match parse_mysql_xml(&first_xml) {
            Ok(result) => result,
            Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
        };
        let (user_result, marker_result) = if is_mysql_row_count_marker(&first_result, &marker) {
            (None, first_result)
        } else {
            let xml = match read_one_mysql_resultset(process).await {
                Ok(xml) => xml,
                Err(error) => {
                    return Err(query_failed_after_mysql_statement(
                        error,
                        refresh_scope,
                        possible_refresh_scope,
                    ));
                }
            };
            let marker_result = match parse_mysql_xml(&xml) {
                Ok(result) => result,
                Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
            };
            let user_result = fill_mysql_empty_result_columns(
                process,
                first_result,
                option_file,
                &statement.sql,
                &statement.kind,
            )
            .await
            .map_err(|error| query_failed_after_change(error, possible_refresh_scope))?;
            (Some(user_result), marker_result)
        };
        let affected_rows = match mysql_row_count_marker(&marker_result, &marker) {
            Ok(rows) => rows,
            Err(error) => return Err(query_failed_after_change(error, possible_refresh_scope)),
        };
        if let Some(result) = user_result {
            last_result_set = Some(result);
        }
        let tag = mysql_command_tag(&statement.kind, affected_rows, last_result_set.as_ref());
        command_tags.push(MysqlCommandEvent {
            kind: statement.kind.clone(),
            target: statement.target.clone(),
            tag,
        });
        refresh_scope = possible_refresh_scope;
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
        let tail = read_pty_all(&mut process.pty)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        trace_mysql_frame("discard tail", tail.len());
        trace_mysql_error(&tail);
        tail
    };

    #[cfg(not(unix))]
    let (_stdout, stderr) =
        tokio::join!(read_all(&mut process.stdout), read_all(&mut process.stderr));
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
    if has_mysql_cli_error(error_bytes) {
        return Err(query_failed_after_change(
            classify_mysql_query_failure(error_bytes),
            scope_before_statement,
        ));
    }
    if !status.success() {
        return Err(query_failed_after_change(
            classify_mysql_query_failure(error_bytes),
            refresh_scope,
        ));
    }

    Ok(MysqlExecutionResult {
        result_set: last_result_set,
        command_tag: aggregate_mysql_command_tag(&command_tags),
        refresh_scope,
    })
}

async fn configure_mysql_session(
    process: &mut MysqlProcess,
    access_mode: AccessMode,
) -> Result<(), DbOperationError> {
    if !access_mode.is_read_only() {
        return Ok(());
    }

    let marker = Uuid::new_v4().simple().to_string();
    write_mysql_statement(process, MYSQL_READ_ONLY_STATEMENT).await?;
    write_mysql_statement(
        process,
        &format!("SELECT '{marker}' AS {MYSQL_SESSION_MARKER_COLUMN}"),
    )
    .await?;
    loop {
        let result = read_one_mysql_resultset(process).await?;
        let result = parse_mysql_xml(&result)?;
        if result.columns.is_empty() && result.values.is_empty() {
            continue;
        }
        return validate_mysql_session_marker(&result, &marker);
    }
}

async fn write_mysql_statement(
    process: &mut MysqlProcess,
    query: &str,
) -> Result<(), DbOperationError> {
    let query = query.trim_end();
    write_mysql_input(process, query.as_bytes()).await?;
    if query.ends_with(';') {
        write_mysql_input(process, b"\n").await
    } else if mysql_statement_has_trailing_line_comment(query) {
        write_mysql_input(process, b"\n;\n").await
    } else {
        write_mysql_input(process, b";\n").await
    }
}

fn mysql_statement_has_trailing_line_comment(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0;
    let mut quote = None;
    let mut line_comment = false;
    while index < bytes.len() {
        if let Some(delimiter) = quote {
            if bytes[index] == b'\\' && delimiter != b'`' {
                index += 2;
            } else if bytes[index] == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                } else {
                    quote = None;
                    index += 1;
                }
            } else {
                index += 1;
            }
            continue;
        }
        if line_comment {
            if bytes[index] == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            quote = Some(bytes[index]);
            index += 1;
        } else if mysql_is_line_comment_start(bytes, index) {
            let comment_start = index;
            index = mysql_skip_line_comment(bytes, index);
            line_comment = !bytes[comment_start..index].contains(&b'\n');
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index = mysql_skip_block_comment(bytes, index);
        } else {
            index += 1;
        }
    }
    line_comment
}

fn mysql_is_line_comment_start(bytes: &[u8], index: usize) -> bool {
    bytes[index] == b'#'
        || (bytes.get(index..index + 2) == Some(b"--")
            && bytes.get(index + 2).is_none_or(u8::is_ascii_whitespace))
}

fn mysql_skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte == b'\n' {
            break;
        }
    }
    index
}

fn mysql_skip_block_comment(bytes: &[u8], index: usize) -> usize {
    let mut cursor = index + 2;
    while cursor + 1 < bytes.len() {
        if bytes[cursor] == b'*' && bytes[cursor + 1] == b'/' {
            return cursor + 2;
        }
        cursor += 1;
    }
    bytes.len()
}

async fn write_mysql_input(
    process: &mut MysqlProcess,
    input: &[u8],
) -> Result<(), DbOperationError> {
    #[cfg(unix)]
    process
        .pty
        .input
        .write_all(input)
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    #[cfg(not(unix))]
    process
        .stdin
        .write_all(input)
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    #[cfg(unix)]
    process
        .pty
        .input
        .flush()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    #[cfg(not(unix))]
    process
        .stdin
        .flush()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
    Ok(())
}

async fn cleanup_mysql_process(process: &mut MysqlProcess) {
    let _ = process.child.kill().await;
    #[cfg(unix)]
    let _ = read_pty_all(&mut process.pty).await;
    #[cfg(not(unix))]
    let _ = tokio::join!(read_all(&mut process.stdout), read_all(&mut process.stderr));
    let _ = process.child.wait().await;
}

async fn read_one_mysql_resultset(process: &mut MysqlProcess) -> Result<Vec<u8>, DbOperationError> {
    #[cfg(unix)]
    {
        return read_one_pty_resultset(&mut process.pty).await;
    }
    #[cfg(not(unix))]
    read_one_mysql_resultset_from_pipes(
        &mut process.stdout,
        &mut process.stderr,
        &mut process.pending,
        &mut process.pending_stderr,
        &mut process.frame_scanner,
    )
    .await
}

async fn mysql_metadata_columns(
    process: &mut MysqlProcess,
    option_file: &std::path::Path,
    query: &str,
    kind: MysqlMetadataFallbackKind,
) -> Result<Vec<String>, DbOperationError> {
    let query = match kind {
        MysqlMetadataFallbackKind::Select => {
            return mysql_metadata_select_columns(process, query).await;
        }
        MysqlMetadataFallbackKind::Show | MysqlMetadataFallbackKind::Describe => {
            query.trim().trim_end_matches(';').trim_end().to_string()
        }
    };
    mysql_metadata_columns_external(option_file, &query).await
}

async fn mysql_metadata_select_columns(
    process: &mut MysqlProcess,
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    let suffix = Uuid::new_v4().simple().to_string();
    let source_alias = format!("__sabiql_metadata_source_{suffix}");
    let marker_alias = format!("__sabiql_metadata_marker_{suffix}");
    let query = mysql_metadata_select_query(query, &source_alias, &marker_alias)?;
    write_mysql_statement(process, &query).await?;
    let xml = match read_one_mysql_resultset(process).await {
        Err(DbOperationError::QueryFailed(details))
            if details
                .to_ascii_lowercase()
                .contains("duplicate column name") =>
        {
            return Err(DbOperationError::UnsupportedOperation(
                "MySQL SELECT metadata fallback does not support duplicate column names"
                    .to_string(),
            ));
        }
        result => result?,
    };
    let result = parse_mysql_xml(&xml)?;
    let row = result.values.first().ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL SELECT metadata fallback returned no synthetic row".to_string(),
        )
    })?;
    if result.values.len() != 1
        || result.columns.is_empty()
        || row.len() != result.columns.len()
        || row.iter().any(|value| !matches!(value, QueryValue::Null))
    {
        return Err(DbOperationError::QueryFailed(
            "MySQL SELECT metadata fallback returned an invalid synthetic row".to_string(),
        ));
    }
    Ok(result.columns)
}

async fn mysql_metadata_columns_external(
    option_file: &std::path::Path,
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    let mut args = mysql_metadata_args(option_file);
    args.push(format!("--execute={query}"));
    let option_file = option_file.to_path_buf();
    let output = run_mysql_command(args, Some(&option_file)).await?;
    if !output.status.success() {
        return Err(classify_mysql_query_failure(&output.stderr));
    }
    parse_mysql_metadata_header(&output.stdout, query)
}

fn parse_mysql_metadata_header(
    output: &[u8],
    query: &str,
) -> Result<Vec<String>, DbOperationError> {
    let query = query.trim_end();
    let query_with_semicolon = format!("{query};");
    let lines = output
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| {
            !line.is_empty()
                && !is_mysql_batch_diagnostic(line)
                && *line != query.as_bytes()
                && *line != query_with_semicolon.as_bytes()
        })
        .collect::<Vec<_>>();
    let header = lines.first().ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL metadata fallback returned no column header".to_string(),
        )
    })?;
    let columns = header
        .split(|byte| *byte == b'\t')
        .map(|column| {
            String::from_utf8(column.to_vec()).map_err(|error| {
                DbOperationError::QueryFailed(format!(
                    "invalid MySQL metadata fallback column name: {error}"
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if columns.is_empty() || columns.iter().any(String::is_empty) {
        return Err(DbOperationError::QueryFailed(
            "MySQL metadata fallback returned an invalid column header".to_string(),
        ));
    }
    if lines.len() != 1 {
        return Err(DbOperationError::QueryFailed(
            "MySQL metadata fallback returned data instead of a header".to_string(),
        ));
    }
    Ok(columns)
}

async fn fill_mysql_empty_result_columns(
    process: &mut MysqlProcess,
    mut result: MysqlResultSet,
    option_file: &std::path::Path,
    query: &str,
    kind: &MysqlStatementKind,
) -> Result<MysqlResultSet, DbOperationError> {
    if !result.columns.is_empty() || !result.values.is_empty() {
        return Ok(result);
    }
    let fallback_kind = mysql_metadata_fallback_kind(kind).ok_or_else(|| {
        DbOperationError::QueryFailed(
            "MySQL empty result has no supported metadata fallback".to_string(),
        )
    })?;
    result.columns = mysql_metadata_columns(process, option_file, query, fallback_kind).await?;
    Ok(result)
}

#[cfg(all(unix, feature = "test-support"))]
pub(super) async fn run_mysql_cli_script_for_test(
    dsn: &str,
    script: &str,
) -> Result<Vec<u8>, DbOperationError> {
    let target = parse_mysql_dsn(dsn)?;
    validate_mysql_values(&target)?;
    validate_mysql_tls_files(&target)?;
    let option_file = MySqlOptionFile::create(&target)?;
    let mut process = MysqlProcess::spawn_with_program(OsStr::new("mysql"), &option_file.path)?;
    let result = async {
        write_mysql_input(&mut process, script.as_bytes()).await?;
        write_mysql_input(&mut process, b"\x04").await?;
        read_pty_all(&mut process.pty)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))
    }
    .await;
    if result.is_err() {
        cleanup_mysql_process(&mut process).await;
    } else {
        let _ = process.child.wait().await;
    }
    result
}
