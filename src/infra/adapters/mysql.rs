use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;

use crate::adapters::csv_export::export_to_downloads;
#[cfg(feature = "test-support")]
use crate::adapters::csv_export::export_to_path;
use crate::app::policy::sql::mysql_statement::{
    MysqlStatement, MysqlStatementKind, classify_mysql_statement, has_mysql_read_only_side_effect,
    mysql_tree_explain_query_kind,
};
use crate::app::policy::write::sql_risk::{
    MultiStatementDecision, evaluate_mysql_multi_statement, mysql_statement_is_data_modifying,
    mysql_statement_is_schema_modifying,
};
use crate::app::ports::outbound::{
    AccessMode, ConnectionProbe, DbOperationError, DdlGenerator, DsnBuilder,
    MYSQL_CLI_VERSION_REQUIRED_MARKER, MYSQL_SERVER_VERSION_REQUIRED_MARKER,
    MYSQL_SQL_MODE_UNSUPPORTED_MARKER, QueryExecutor, SqlDialect,
};
use crate::domain::connection::{ConnectionProfile, DatabaseType};
use crate::domain::{
    CommandTag, QueryResult, QuerySource, QueryValue, RefreshScope, Table, WriteExecutionResult,
};

mod cli;
mod dsn;
mod metadata;
mod option_file;
mod sql;

use cli::{
    MYSQL_QUERY_TIMEOUT, MysqlMetadataSession, MysqlProcess, MysqlResultSet,
    classify_mysql_probe_failure, clean_stderr, export_mysql_csv_to_file, mysql_metadata_columns,
    mysql_probe_args, run_mysql_adhoc_with_program_and_statements, run_mysql_command,
    run_mysql_single_statement,
};
use dsn::{
    MySqlDsn, build_mysql_dsn, parse_mysql_dsn, validate_mysql_tls_files, validate_mysql_values,
};
use option_file::MySqlOptionFile;

pub struct MySqlAdapter;

impl MySqlAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MySqlAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(unix, feature = "test-support"))]
#[doc(hidden)]
pub async fn run_mysql_cli_script_for_test(
    dsn: &str,
    script: &str,
) -> Result<Vec<u8>, DbOperationError> {
    cli::run_mysql_cli_script_for_test(dsn, script).await
}

#[cfg(feature = "test-support")]
#[doc(hidden)]
/// Runs the export process without client-side query policy validation so integration tests can
/// verify that the MySQL read-only session rejects a side effect at the server boundary.
pub async fn export_mysql_csv_to_path_for_test(
    dsn: &str,
    query: &str,
    path: PathBuf,
) -> Result<PathBuf, DbOperationError> {
    let target = parse_mysql_dsn(dsn)?;
    validate_mysql_values(&target)?;
    validate_mysql_tls_files(&target)?;
    let query = query.to_string();
    export_to_path(path, move |temporary_path| async move {
        export_mysql_csv_to_file(target, &query, temporary_path).await
    })
    .await
}

#[async_trait]
impl QueryExecutor for MySqlAdapter {
    async fn execute_preview(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult, DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let preview = metadata::fetch_preview_metadata(dsn, schema, table).await?;
        let query = metadata::build_preview_query(
            schema,
            table,
            &preview.order_columns,
            &preview.visible_columns,
            &preview.identity_columns,
            limit,
            offset,
        );
        let display_query = metadata::build_preview_query(
            schema,
            table,
            &preview.order_columns,
            &preview.visible_columns,
            &[],
            limit,
            offset,
        );
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        let statements =
            validate_mysql_multi_query(&query, target.database.as_deref(), AccessMode::ReadWrite)?;
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(
            &option_file.path,
            &query,
            &statements,
            AccessMode::ReadWrite,
        )
        .await;
        drop(option_file);
        let result_set = result?.result_set.ok_or_else(|| {
            DbOperationError::MetadataParseFailed(
                "MySQL preview query returned no result set".to_string(),
            )
        })?;
        let values = metadata::convert_preview_values(
            &result_set,
            &preview.visible_columns,
            &preview.identity_columns,
        )?;
        let elapsed = start.elapsed().as_millis() as u64;

        let mut query_result = QueryResult::success_with_values(
            display_query,
            preview
                .visible_columns
                .iter()
                .map(|column| column.name.clone())
                .collect(),
            values.visible,
            elapsed,
            QuerySource::Preview,
        );
        if let Some(identity_values) = values.identity {
            query_result = query_result.with_explicit_row_identity(
                preview
                    .identity_columns
                    .iter()
                    .map(|column| column.name.clone())
                    .collect(),
                identity_values,
            );
        }
        Ok(query_result)
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;

        if mysql_tree_explain_query_kind(query).is_some() {
            #[expect(
                clippy::disallowed_methods,
                reason = "infra measures mysql execution time at the I/O boundary"
            )]
            let start = Instant::now();
            let option_file = MySqlOptionFile::create(&target)?;
            let result = run_mysql_single_statement(&option_file.path, query, access_mode).await;
            drop(option_file);
            let result_set = result?;
            return Ok(QueryResult::success_with_values(
                query.to_string(),
                result_set.columns,
                result_set.values,
                start.elapsed().as_millis() as u64,
                QuerySource::Adhoc,
            ));
        }

        let statements =
            validate_mysql_multi_query(query, target.database.as_deref(), access_mode)?;

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, query, &statements, access_mode).await;
        drop(option_file);
        let execution = result?;
        let elapsed = start.elapsed().as_millis() as u64;
        let mut result = match execution.result_set {
            Some(result_set) => QueryResult::success_with_values(
                query.to_string(),
                result_set.columns,
                result_set.values,
                elapsed,
                QuerySource::Adhoc,
            ),
            None => QueryResult::success(
                query.to_string(),
                Vec::new(),
                Vec::new(),
                elapsed,
                QuerySource::Adhoc,
            ),
        };
        if let Some(tag) = execution.command_tag {
            result = result.with_command_tag(tag);
        }
        Ok(result.with_refresh_scope(execution.refresh_scope))
    }

    async fn execute_write(
        &self,
        dsn: &str,
        query: &str,
        access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        let statements =
            validate_mysql_multi_query(query, target.database.as_deref(), access_mode)?;

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, query, &statements, access_mode).await;
        drop(option_file);
        let execution = result?;
        let affected_rows = execution
            .command_tag
            .and_then(|tag| tag.affected_rows())
            .ok_or_else(|| {
                DbOperationError::CommandTagParseFailed(
                    "MySQL write did not return an affected row count".to_string(),
                )
            })?;
        let affected_rows = usize::try_from(affected_rows).map_err(|_| {
            DbOperationError::CommandTagParseFailed(
                "MySQL affected row count does not fit in usize".to_string(),
            )
        })?;

        Ok(WriteExecutionResult {
            affected_rows,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn count_query_rows(&self, dsn: &str, query: &str) -> Result<usize, DbOperationError> {
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        validate_mysql_export_query(query, target.database.as_deref())?;

        let result = self.execute_adhoc(dsn, query, AccessMode::ReadOnly).await?;
        let value = result
            .values()
            .first()
            .and_then(|row| row.first())
            .and_then(QueryValue::as_str)
            .ok_or_else(|| {
                DbOperationError::QueryFailed(
                    "MySQL row count query returned an invalid result".to_string(),
                )
            })?;
        value.parse::<usize>().map_err(|_| {
            DbOperationError::QueryFailed("MySQL row count was not an integer".to_string())
        })
    }

    async fn export_to_csv(
        &self,
        dsn: &str,
        query: &str,
        file_name: &str,
    ) -> Result<std::path::PathBuf, DbOperationError> {
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        validate_mysql_export_query(query, target.database.as_deref())?;

        let query = query.to_string();
        export_to_downloads(file_name, move |path| async move {
            export_mysql_csv_to_file(target, &query, path).await
        })
        .await
    }
}

impl DdlGenerator for MySqlAdapter {
    fn generate_ddl(&self, _database_type: DatabaseType, table: &Table) -> String {
        table.source_ddl().unwrap_or_default().to_string()
    }
}

impl SqlDialect for MySqlAdapter {
    fn build_explain_sql(&self, _database_type: DatabaseType, query: &str) -> Option<String> {
        sql::build_explain_sql(query)
    }

    fn build_explain_analyze_sql(
        &self,
        _database_type: DatabaseType,
        query: &str,
    ) -> Option<String> {
        sql::build_explain_analyze_sql(query)
    }

    fn build_update_sql(
        &self,
        _database_type: DatabaseType,
        schema: &str,
        table: &str,
        column: &str,
        new_value: &QueryValue,
        pk_pairs: &[(String, QueryValue)],
    ) -> String {
        sql::build_update_sql(schema, table, column, new_value, pk_pairs)
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        sql::build_bulk_delete_sql(schema, table, pk_pairs_per_row)
    }
}

impl DsnBuilder for MySqlAdapter {
    fn build_dsn(&self, profile: &ConnectionProfile) -> String {
        let config = profile
            .mysql_config()
            .expect("MySQL profile requires MySQL config");
        build_mysql_dsn(config)
    }
}

#[async_trait]
impl ConnectionProbe for MySqlAdapter {
    async fn probe(&self, dsn: &str) -> Result<(), DbOperationError> {
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;
        self.check_cli_version().await?;

        let option_file = MySqlOptionFile::create(&target)?;
        let result = self.run_probe(&option_file.path).await;
        drop(option_file);
        let output = result?;

        if !output.status.success() {
            return Err(classify_mysql_probe_failure(clean_stderr(&output.stderr)));
        }

        let response: MySqlProbeResponse = serde_json::from_slice(&output.stdout)?;
        let _ = (&response.database, &response.user);
        validate_server_version(&response.version)?;
        validate_sql_mode(&response.sql_mode)?;
        Ok(())
    }

    async fn fetch_databases(&self, dsn: &str) -> Result<Vec<String>, DbOperationError> {
        let mut target = parse_mysql_dsn(dsn)?;
        target.database = None;
        validate_mysql_values(&target)?;
        self.check_cli_version().await?;

        let option_file = MySqlOptionFile::create(&target)?;
        let statements = validate_mysql_multi_query("SHOW DATABASES", None, AccessMode::ReadWrite)?;
        let result = run_mysql_adhoc(
            &option_file.path,
            "SHOW DATABASES",
            &statements,
            AccessMode::ReadWrite,
        )
        .await;
        drop(option_file);
        result.map(|execution| {
            execution
                .result_set
                .unwrap_or(MysqlResultSet {
                    columns: Vec::new(),
                    values: Vec::new(),
                })
                .values
                .into_iter()
                .filter_map(|mut row| row.drain(..).next())
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
    }
}

impl MySqlAdapter {
    async fn check_cli_version(&self) -> Result<(), DbOperationError> {
        let output = run_mysql_command(["--version"], None).await?;
        let version_output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() || !is_oracle_mysql_cli_84_version(&version_output) {
            return Err(DbOperationError::UnsupportedOperation(format!(
                "{MYSQL_CLI_VERSION_REQUIRED_MARKER}: {}",
                version_output.trim()
            )));
        }
        Ok(())
    }

    async fn run_probe(
        &self,
        option_file: &PathBuf,
    ) -> Result<std::process::Output, DbOperationError> {
        let args = mysql_probe_args(option_file);
        run_mysql_command(args, Some(option_file)).await
    }
}

#[derive(Debug, Deserialize)]
struct MySqlProbeResponse {
    database: Option<String>,
    user: String,
    version: String,
    sql_mode: String,
}

fn contains_unsupported_mysql_product(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["mariadb", "percona", "tidb", "vitess", "aurora"]
        .iter()
        .any(|product| lower.contains(product))
}

fn is_oracle_mysql_cli_84_version(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("mysql")
        && !contains_unsupported_mysql_product(value)
        && version_major_minor(value) == Some((8, 4))
}

fn is_oracle_mysql_server_84_version(value: &str) -> bool {
    !contains_unsupported_mysql_product(value) && version_major_minor(value) == Some((8, 4))
}

fn validate_server_version(version: &str) -> Result<(), DbOperationError> {
    if is_oracle_mysql_server_84_version(version) {
        Ok(())
    } else {
        Err(DbOperationError::UnsupportedOperation(format!(
            "{MYSQL_SERVER_VERSION_REQUIRED_MARKER}: {version}"
        )))
    }
}

fn validate_sql_mode(sql_mode: &str) -> Result<(), DbOperationError> {
    let unsupported = sql_mode.split(',').map(str::trim).any(|mode| {
        mode.eq_ignore_ascii_case("NO_BACKSLASH_ESCAPES")
            || mode.eq_ignore_ascii_case("ANSI_QUOTES")
    });
    if unsupported {
        Err(DbOperationError::UnsupportedOperation(format!(
            "{MYSQL_SQL_MODE_UNSUPPORTED_MARKER}: {sql_mode}"
        )))
    } else {
        Ok(())
    }
}

fn version_major_minor(value: &str) -> Option<(u32, u32)> {
    let mut numbers = value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok());
    Some((numbers.next()?, numbers.next()?))
}

enum MysqlMetadataFallbackKind {
    Select,
    Show,
    Describe,
}

fn mysql_metadata_fallback_kind(kind: &MysqlStatementKind) -> Option<MysqlMetadataFallbackKind> {
    match kind {
        MysqlStatementKind::Select => Some(MysqlMetadataFallbackKind::Select),
        MysqlStatementKind::Show => Some(MysqlMetadataFallbackKind::Show),
        MysqlStatementKind::Describe => Some(MysqlMetadataFallbackKind::Describe),
        _ => None,
    }
}

fn mysql_metadata_select_query(
    query: &str,
    source_alias: &str,
    marker_alias: &str,
) -> Result<String, DbOperationError> {
    let query = query.trim().trim_end_matches(';').trim_end();
    if query.is_empty() {
        return Err(DbOperationError::QueryFailed(
            "MySQL empty SELECT cannot be used for metadata fallback".to_string(),
        ));
    }
    if mysql_metadata_select_has_unproven_function(query) {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL SELECT metadata fallback cannot prove that function calls are side-effect free"
                .to_string(),
        ));
    }
    if has_mysql_read_only_side_effect(query)
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?
    {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL SELECT metadata fallback cannot prove that the query is side-effect free"
                .to_string(),
        ));
    }
    Ok(sql::build_metadata_select_query(
        query,
        source_alias,
        marker_alias,
    ))
}

fn mysql_metadata_select_has_unproven_function(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#'
            || (bytes.get(index..index + 2) == Some(b"--")
                && bytes.get(index + 2).is_none_or(u8::is_ascii_whitespace))
        {
            index = bytes[index..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| index + offset + 1);
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            index = bytes
                .get(index + 2..)
                .and_then(|rest| rest.windows(2).position(|window| window == b"*/"))
                .map_or(bytes.len(), |offset| index + offset + 4);
            continue;
        }
        if bytes[index] == b'@' {
            return true;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            let quote = bytes[index];
            index = skip_mysql_metadata_quoted(bytes, index, quote);
            let next = skip_mysql_metadata_trivia(bytes, index);
            if quote == b'`' && bytes.get(next) == Some(&b'(') {
                return true;
            }
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || matches!(bytes[index], b'_' | b'$') {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
            {
                index += 1;
            }
            let next = skip_mysql_metadata_trivia(bytes, index);
            if bytes.get(next) == Some(&b'(') {
                let name = &sql[start..index];
                let cte_column_list = mysql_metadata_is_cte_column_list(sql, next);
                let qualified = mysql_metadata_has_qualifier(bytes, start);
                if !cte_column_list
                    && (qualified
                        || !name.eq_ignore_ascii_case("SLEEP")
                            && !matches!(
                                name.to_ascii_uppercase().as_str(),
                                "AS" | "CASE" | "IN" | "EXISTS" | "OVER"
                            ))
                {
                    return true;
                }
            }
            continue;
        }
        index += 1;
    }
    false
}

fn skip_mysql_metadata_trivia(bytes: &[u8], mut index: usize) -> usize {
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'#')
            || (bytes.get(index..index + 2) == Some(b"--")
                && bytes.get(index + 2).is_some_and(u8::is_ascii_whitespace))
        {
            index = bytes[index..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| index + offset + 1);
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            index = bytes
                .get(index + 2..)
                .and_then(|rest| rest.windows(2).position(|window| window == b"*/"))
                .map_or(bytes.len(), |offset| index + offset + 4);
            continue;
        }
        return index;
    }
}

fn mysql_metadata_has_qualifier(bytes: &[u8], end: usize) -> bool {
    let mut index = 0;
    let mut previous = None;
    while index < end {
        if bytes[index] == b'#'
            || (bytes.get(index..index + 2) == Some(b"--")
                && bytes.get(index + 2).is_some_and(u8::is_ascii_whitespace))
            || bytes.get(index..index + 2) == Some(b"/*")
        {
            let next = skip_mysql_metadata_trivia(bytes, index);
            if next == index {
                index += 1;
            } else {
                index = next;
            }
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_mysql_metadata_quoted(bytes, index, bytes[index]);
            previous = Some(b'\'');
        } else if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else {
            previous = Some(bytes[index]);
            index += 1;
        }
    }
    previous == Some(b'.')
}

fn mysql_metadata_is_cte_column_list(sql: &str, candidate_open: usize) -> bool {
    let bytes = sql.as_bytes();
    let mut index = skip_mysql_metadata_trivia(bytes, 0);
    let Some(after_with) = mysql_metadata_keyword_end(bytes, index, "WITH") else {
        return false;
    };
    index = skip_mysql_metadata_trivia(bytes, after_with);
    if let Some(after_recursive) = mysql_metadata_keyword_end(bytes, index, "RECURSIVE") {
        index = skip_mysql_metadata_trivia(bytes, after_recursive);
    }

    loop {
        index = match mysql_metadata_cte_name_end(bytes, index) {
            Some(end) => skip_mysql_metadata_trivia(bytes, end),
            None => return false,
        };
        let mut column_list = None;
        if bytes.get(index) == Some(&b'(') {
            let Some(end) = mysql_metadata_parenthesized_end(bytes, index) else {
                return false;
            };
            column_list = Some(index);
            index = skip_mysql_metadata_trivia(bytes, end);
        }
        let Some(after_as) = mysql_metadata_keyword_end(bytes, index, "AS") else {
            return false;
        };
        index = skip_mysql_metadata_trivia(bytes, after_as);
        let Some(body_end) = bytes
            .get(index)
            .filter(|byte| **byte == b'(')
            .and_then(|_| mysql_metadata_parenthesized_end(bytes, index))
        else {
            return false;
        };
        if column_list == Some(candidate_open) {
            return true;
        }
        index = skip_mysql_metadata_trivia(bytes, body_end);
        if bytes.get(index) != Some(&b',') {
            return false;
        }
        index = skip_mysql_metadata_trivia(bytes, index + 1);
    }
}

fn mysql_metadata_cte_name_end(bytes: &[u8], index: usize) -> Option<usize> {
    if bytes.get(index) == Some(&b'`') {
        Some(skip_mysql_metadata_quoted(bytes, index, b'`'))
    } else if bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
    {
        Some(
            index
                + 1
                + bytes[index + 1..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
                    .count(),
        )
    } else {
        None
    }
}

fn mysql_metadata_keyword_end(bytes: &[u8], index: usize, keyword: &str) -> Option<usize> {
    let end = index.checked_add(keyword.len())?;
    if bytes
        .get(index..end)?
        .eq_ignore_ascii_case(keyword.as_bytes())
        && !bytes
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    {
        Some(end)
    } else {
        None
    }
}

fn mysql_metadata_parenthesized_end(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut index = open;
    while index < bytes.len() {
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_mysql_metadata_quoted(bytes, index, bytes[index]);
            continue;
        }
        let next = skip_mysql_metadata_trivia(bytes, index);
        if next != index {
            index = next;
            continue;
        }
        match bytes[index] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn skip_mysql_metadata_quoted(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' && quote != b'`' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == quote {
            if bytes.get(index + 1) == Some(&quote) {
                index += 2;
            } else {
                return index + 1;
            }
        } else {
            index += 1;
        }
    }
    bytes.len()
}

#[derive(Debug, PartialEq, Eq)]
struct MysqlExecutionResult {
    result_set: Option<MysqlResultSet>,
    command_tag: Option<CommandTag>,
    refresh_scope: RefreshScope,
}

struct MysqlCommandEvent {
    kind: MysqlStatementKind,
    target: Option<String>,
    tag: CommandTag,
}

fn validate_mysql_multi_query(
    query: &str,
    selected_database: Option<&str>,
    access_mode: AccessMode,
) -> Result<Vec<MysqlStatement>, DbOperationError> {
    let decision = evaluate_mysql_multi_statement(query, selected_database);
    let (statements, risk) = match decision {
        MultiStatementDecision::Allow { statements, risk } => (statements, risk),
        MultiStatementDecision::Block { reason } => {
            return Err(DbOperationError::UnsupportedOperation(reason));
        }
    };
    if access_mode.is_read_only() && !risk.read_only_allowed {
        return Err(DbOperationError::PermissionDenied(
            "read-only mode blocks MySQL write statements".to_string(),
        ));
    }
    statements
        .iter()
        .map(|statement| {
            classify_mysql_statement(statement)
                .map_err(|error| DbOperationError::UnsupportedOperation(error.to_string()))
        })
        .collect()
}

fn mysql_metadata_fallback_has_unsupported_session_state(statements: &[MysqlStatement]) -> bool {
    let mut temporary_table_created = false;
    for statement in statements {
        if temporary_table_created
            && matches!(
                statement.kind,
                MysqlStatementKind::Show | MysqlStatementKind::Describe
            )
        {
            return true;
        }
        if matches!(
            statement.kind,
            MysqlStatementKind::CreateTable { temporary: true }
        ) {
            temporary_table_created = true;
        }
    }
    false
}

fn validate_mysql_export_query(
    query: &str,
    selected_database: Option<&str>,
) -> Result<(), DbOperationError> {
    let statements = validate_mysql_multi_query(query, selected_database, AccessMode::ReadOnly)?;
    if statements.len() != 1
        || !matches!(
            statements[0].kind,
            MysqlStatementKind::Select
                | MysqlStatementKind::Table
                | MysqlStatementKind::Show
                | MysqlStatementKind::Describe
        )
    {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL CSV export supports a single read-only result query".to_string(),
        ));
    }
    Ok(())
}

async fn run_mysql_adhoc(
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

fn query_failed_after_change(
    error: DbOperationError,
    refresh_scope: RefreshScope,
) -> DbOperationError {
    if refresh_scope == RefreshScope::None {
        error
    } else {
        DbOperationError::QueryFailedAfterChange {
            source: Arc::new(error),
            refresh_scope,
        }
    }
}

fn query_failed_after_mysql_statement(
    error: DbOperationError,
    refresh_scope: RefreshScope,
    possible_refresh_scope: RefreshScope,
) -> DbOperationError {
    let refresh_scope = if is_mysql_statement_failure(&error) {
        refresh_scope
    } else {
        possible_refresh_scope
    };
    query_failed_after_change(error, refresh_scope)
}

fn is_mysql_statement_failure(error: &DbOperationError) -> bool {
    matches!(
        error,
        DbOperationError::PermissionDenied(_)
            | DbOperationError::ForeignKeyViolation(_)
            | DbOperationError::UniqueViolation(_)
            | DbOperationError::LockTimeout(_)
            | DbOperationError::ObjectMissing(_)
            | DbOperationError::QueryFailed(_)
            | DbOperationError::Canceled(_)
    )
}

fn is_mysql_row_count_marker(result: &MysqlResultSet, marker: &str) -> bool {
    result.columns == ["__sabiql_marker", "affected_rows"]
        && result.values.len() == 1
        && result.values[0].first().and_then(QueryValue::as_str) == Some(marker)
}

fn mysql_row_count_marker(result: &MysqlResultSet, marker: &str) -> Result<i64, DbOperationError> {
    if !is_mysql_row_count_marker(result, marker) || result.values[0].len() != 2 {
        return Err(DbOperationError::QueryFailed(
            "mysql ROW_COUNT marker did not match the executed statement".to_string(),
        ));
    }
    let value = result.values[0][1].as_str().ok_or_else(|| {
        DbOperationError::QueryFailed("mysql ROW_COUNT marker was NULL".to_string())
    })?;
    value.parse::<i64>().map_err(|_| {
        DbOperationError::QueryFailed("mysql ROW_COUNT marker was not an integer".to_string())
    })
}

fn mysql_command_tag(
    kind: &MysqlStatementKind,
    affected_rows: i64,
    user_result: Option<&MysqlResultSet>,
) -> CommandTag {
    let rows = || u64::try_from(affected_rows.max(0)).unwrap_or(0);
    match kind {
        MysqlStatementKind::Select
        | MysqlStatementKind::Table
        | MysqlStatementKind::Show
        | MysqlStatementKind::Describe => {
            CommandTag::Select(user_result.map_or(0, |result| result.values.len() as u64))
        }
        MysqlStatementKind::Insert | MysqlStatementKind::Replace => CommandTag::Insert(rows()),
        MysqlStatementKind::Update { .. } => CommandTag::Update(rows()),
        MysqlStatementKind::Delete { .. } => CommandTag::Delete(rows()),
        MysqlStatementKind::CreateTable { temporary: true } => {
            CommandTag::Other("CREATE TEMPORARY TABLE".to_string())
        }
        MysqlStatementKind::CreateTable { temporary: false } => {
            CommandTag::Create("TABLE".to_string())
        }
        MysqlStatementKind::AlterTable => CommandTag::Alter("TABLE".to_string()),
        MysqlStatementKind::DropTable { temporary: true } => {
            CommandTag::Other("DROP TEMPORARY TABLE".to_string())
        }
        MysqlStatementKind::DropTable { temporary: false } => CommandTag::Drop("TABLE".to_string()),
        MysqlStatementKind::TruncateTable => CommandTag::Truncate,
        MysqlStatementKind::CreateView => CommandTag::Create("VIEW".to_string()),
        MysqlStatementKind::DropView => CommandTag::Drop("VIEW".to_string()),
        MysqlStatementKind::CreateIndex => CommandTag::Create("INDEX".to_string()),
        MysqlStatementKind::DropIndex => CommandTag::Drop("INDEX".to_string()),
        MysqlStatementKind::Begin | MysqlStatementKind::StartTransaction => CommandTag::Begin,
        MysqlStatementKind::Commit => CommandTag::Commit,
        MysqlStatementKind::Rollback | MysqlStatementKind::RollbackToSavepoint => {
            CommandTag::Rollback
        }
        MysqlStatementKind::Savepoint => CommandTag::Other("SAVEPOINT".to_string()),
        MysqlStatementKind::ReleaseSavepoint => CommandTag::Other("RELEASE SAVEPOINT".to_string()),
    }
}

fn mysql_refresh_scope(kind: &MysqlStatementKind) -> RefreshScope {
    if mysql_statement_is_schema_modifying(kind) {
        RefreshScope::Metadata
    } else if mysql_statement_is_data_modifying(kind) {
        RefreshScope::Data
    } else {
        RefreshScope::None
    }
}

fn mysql_statement_is_persistent_schema_change(kind: &MysqlStatementKind) -> bool {
    mysql_statement_is_schema_modifying(kind)
        && !matches!(
            kind,
            MysqlStatementKind::CreateTable { temporary: true }
                | MysqlStatementKind::DropTable { temporary: true }
        )
}

#[derive(Default)]
struct MysqlPendingTransactionTags {
    data: Vec<CommandTag>,
    savepoints: Vec<(String, usize)>,
}

fn apply_pending_mysql_data(
    pending: MysqlPendingTransactionTags,
    committed_data: &mut Option<CommandTag>,
) {
    if let Some(tag) = pending.data.last() {
        *committed_data = Some(tag.clone());
    }
}

fn aggregate_mysql_command_tag(events: &[MysqlCommandEvent]) -> Option<CommandTag> {
    let mut committed_schema = None;
    let mut committed_data = None;
    let mut pending = None;
    let mut last_tag = None;

    for event in events {
        last_tag = Some(event.tag.clone());
        match &event.kind {
            MysqlStatementKind::Begin | MysqlStatementKind::StartTransaction => {
                pending = Some(MysqlPendingTransactionTags::default());
            }
            MysqlStatementKind::Commit => {
                if let Some(transaction) = pending.take() {
                    apply_pending_mysql_data(transaction, &mut committed_data);
                }
            }
            MysqlStatementKind::Rollback => {
                pending = None;
            }
            MysqlStatementKind::Savepoint => {
                if let Some(transaction) = pending.as_mut()
                    && let Some(name) = event.target.as_deref()
                {
                    transaction
                        .savepoints
                        .retain(|(current, _)| !current.eq_ignore_ascii_case(name));
                    transaction
                        .savepoints
                        .push((name.to_string(), transaction.data.len()));
                }
            }
            MysqlStatementKind::RollbackToSavepoint => {
                if let Some(transaction) = pending.as_mut()
                    && let Some(name) = event.target.as_deref()
                    && let Some(index) = transaction
                        .savepoints
                        .iter()
                        .position(|(current, _)| current.eq_ignore_ascii_case(name))
                {
                    transaction.data.truncate(transaction.savepoints[index].1);
                    transaction.savepoints.truncate(index + 1);
                }
            }
            MysqlStatementKind::ReleaseSavepoint => {
                if let Some(transaction) = pending.as_mut()
                    && let Some(name) = event.target.as_deref()
                    && let Some(index) = transaction
                        .savepoints
                        .iter()
                        .position(|(current, _)| current.eq_ignore_ascii_case(name))
                {
                    transaction.savepoints.remove(index);
                }
            }
            MysqlStatementKind::CreateTable { temporary: true }
            | MysqlStatementKind::DropTable { temporary: true } => {}
            kind if mysql_statement_is_persistent_schema_change(kind) => {
                if let Some(transaction) = pending.take() {
                    apply_pending_mysql_data(transaction, &mut committed_data);
                }
                committed_schema = Some(event.tag.clone());
            }
            kind if mysql_statement_is_data_modifying(kind) => {
                if let Some(transaction) = pending.as_mut() {
                    transaction.data.push(event.tag.clone());
                } else {
                    committed_data = Some(event.tag.clone());
                }
            }
            _ => {}
        }
    }

    committed_schema.or(committed_data).or(last_tag)
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

fn validate_mode_probe(result: &MysqlResultSet, marker: &str) -> Result<(), DbOperationError> {
    if result.values.len() != 1 || result.columns != ["__sabiql_probe", "__sabiql_sql_mode"] {
        return Err(DbOperationError::QueryFailed(
            "mysql sql_mode probe returned an unexpected result".to_string(),
        ));
    }
    let values = &result.values[0];
    if values.len() != 2 {
        return Err(DbOperationError::QueryFailed(
            "mysql sql_mode probe returned an unexpected result".to_string(),
        ));
    }
    if values[0].as_str() != Some(marker) {
        return Err(DbOperationError::QueryFailed(
            "mysql sql_mode probe marker did not match".to_string(),
        ));
    }
    let mode = values[1].as_str().ok_or_else(|| {
        DbOperationError::QueryFailed("mysql sql_mode probe returned no mode".to_string())
    })?;
    validate_sql_mode(mode)
}

#[cfg(test)]
mod probe_tests {
    use sabiql_app::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};

    use super::*;

    #[test]
    fn metadata_only_select_rejects_known_side_effects() {
        for query in [
            "SELECT value FROM items FOR UPDATE",
            "SELECT GET_LOCK('sabiql', 0)",
            "SELECT @value := 1",
        ] {
            assert!(
                mysql_metadata_select_query(query, "__source", "__marker").is_err(),
                "{query}"
            );
        }
        assert!(mysql_metadata_select_query(
            "WITH cte_rows AS (SELECT 1 AS first_alias) SELECT first_alias FROM cte_rows WHERE FALSE",
            "__source",
            "__marker"
        )
        .is_ok());
        assert!(
            mysql_metadata_select_query(
                "SELECT CONCAT('a', 'b') AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT CONCAT/**/('a', 'b') AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT sabiql_test.user_function() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT sabiql_test/**/.user_function() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT `user_function`/**/() AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT @session_value AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT INTERVAL(10, 1, 5) AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_err()
        );
        assert!(
            mysql_metadata_select_query(
            "WITH cte_rows(first_alias) AS (SELECT 1) SELECT first_alias FROM cte_rows WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
        assert!(
            mysql_metadata_select_query(
                "SELECT CASE (1) WHEN 1 THEN 'x' ELSE 'y' END AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
        for query in [
            "SELECT CAST(1 AS CHAR) AS value WHERE FALSE",
            "SELECT CONVERT(1, CHAR) AS value WHERE FALSE",
            "SELECT EXTRACT(YEAR FROM CURRENT_DATE) AS value WHERE FALSE",
        ] {
            assert!(
                mysql_metadata_select_query(query, "__source", "__marker").is_err(),
                "{query}"
            );
        }
        assert!(
            mysql_metadata_select_query(
                "SELECT SLEEP(1) AS value WHERE FALSE",
                "__source",
                "__marker"
            )
            .is_ok()
        );
    }

    #[test]
    fn validates_supported_versions_and_sql_modes() {
        assert!(is_oracle_mysql_cli_84_version("mysql  Ver 8.4.3 for macos"));
        assert!(!is_oracle_mysql_cli_84_version(
            "mysql  Ver 8.0.36 for macos"
        ));
        assert!(!is_oracle_mysql_cli_84_version(
            "mysql  Ver 15.1 Distrib 10.11.8-MariaDB"
        ));
        assert!(!is_oracle_mysql_cli_84_version(
            "mysql  Ver 8.4.3-3 for Linux (Percona Server)"
        ));
        assert!(is_oracle_mysql_server_84_version("8.4.3"));
        assert!(!is_oracle_mysql_server_84_version("8.4.3-TiDB"));
        assert!(!is_oracle_mysql_server_84_version("8.4.3-Percona"));
        assert!(validate_sql_mode("STRICT_TRANS_TABLES").is_ok());
        assert!(validate_sql_mode("STRICT_TRANS_TABLES,ANSI_QUOTES").is_err());
    }

    #[test]
    fn uses_tcp_and_keeps_defaults_file_first() {
        let args = mysql_probe_args(std::path::Path::new("/tmp/sabiql-mysql.cnf"));

        assert_eq!(args[0], "--defaults-file=/tmp/sabiql-mysql.cnf");
        assert_eq!(args[1], "--no-login-paths");
        assert_eq!(args[2], "--protocol=TCP");
        assert!(args.contains(&"--batch".to_string()));
        assert!(args.contains(&"--raw".to_string()));
        assert!(args.contains(&"--skip-column-names".to_string()));
        assert!(args.contains(&"--binary-mode".to_string()));
        assert!(args.contains(&"--skip-reconnect".to_string()));
        assert!(
            args.last()
                .unwrap()
                .starts_with("--execute=SELECT JSON_OBJECT")
        );
    }

    #[test]
    fn arguments_do_not_contain_credentials() {
        let args = mysql_probe_args(std::path::Path::new("/tmp/sabiql-mysql.cnf"));

        assert!(
            args.iter()
                .all(|argument| { !argument.contains("password") && !argument.contains("secret") })
        );
    }

    #[test]
    fn classifies_mysql_cli_timeout_errno_before_connection_refusal() {
        assert!(matches!(
            classify_mysql_probe_failure(
                "ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (110)"
                    .to_string()
            ),
            DbOperationError::Timeout(_)
        ));
        assert!(matches!(
            classify_mysql_probe_failure(
                "ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (111)"
                    .to_string()
            ),
            DbOperationError::ConnectionFailed(_)
        ));
    }

    #[test]
    fn classifies_mysql_tls_probe_failure_for_connection_error() {
        let error = classify_mysql_probe_failure(
            "ERROR 2026 (HY000): SSL connection error: error:0A000086:SSL routines::certificate verify failed"
                .to_string(),
        );

        assert_eq!(
            ConnectionErrorInfo::from_db_operation_error(&error).kind,
            ConnectionErrorKind::MySqlTlsHandshakeFailed
        );
    }
}
