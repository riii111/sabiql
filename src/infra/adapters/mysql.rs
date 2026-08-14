use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};

use async_trait::async_trait;
use serde::Deserialize;
#[cfg(unix)]
use tokio::fs::File as TokioFile;
#[cfg(not(unix))]
use tokio::io::AsyncRead;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;
use tokio::process::Command;
#[cfg(not(unix))]
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use tokio::time::timeout;
use url::Url;
use uuid::Uuid;

#[cfg(any(test, feature = "test-support"))]
use crate::adapters::csv_export::export_to_path;
use crate::adapters::csv_export::{CsvFileWriter, export_to_downloads};
#[cfg(test)]
use crate::app::policy::sql::mysql_statement::split_mysql_statements;
use crate::app::policy::sql::mysql_statement::{
    MysqlStatement, MysqlStatementKind, classify_mysql_statement, mysql_explain_rejection_message,
    mysql_tree_explain_query_kind,
};
use crate::app::policy::write::sql_risk::{
    MultiStatementDecision, evaluate_mysql_multi_statement, mysql_statement_is_data_modifying,
    mysql_statement_is_schema_modifying,
};
use crate::app::ports::outbound::{
    AccessMode, ConnectionProbe, DatabaseCli, DbOperationError, DdlGenerator, DsnBuilder,
    MYSQL_CLI_VERSION_REQUIRED_MARKER, MYSQL_SERVER_VERSION_REQUIRED_MARKER,
    MYSQL_SQL_MODE_UNSUPPORTED_MARKER, QueryExecutor, SqlDialect,
};
use crate::domain::connection::{
    ConnectionProfile, DatabaseType, MySqlConnectionConfig, MySqlSslMode,
};
use crate::domain::{
    CommandTag, QueryResult, QuerySource, QueryValue, RefreshScope, Table, WriteExecutionResult,
};

mod metadata;

pub struct MySqlAdapter;

const MYSQL_PROBE_TIMEOUT: Duration = Duration::from_secs(11);
const MYSQL_QUERY_TIMEOUT: Duration = Duration::from_secs(31);
const MYSQL_EXPORT_TIMEOUT: Duration = Duration::from_secs(MYSQL_QUERY_TIMEOUT.as_secs() * 10);
const MYSQL_PROBE_QUERY: &str = "SELECT JSON_OBJECT('database', DATABASE(), 'user', CURRENT_USER(), 'version', VERSION(), 'sql_mode', @@SESSION.sql_mode)";
const MYSQL_READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";
const MYSQL_SESSION_MARKER_COLUMN: &str = "__sabiql_session_marker";

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
        if mysql_explain_rejection_message(query).is_some() {
            return None;
        }
        Some(format!("EXPLAIN FORMAT=TREE {query}"))
    }

    fn build_explain_analyze_sql(
        &self,
        _database_type: DatabaseType,
        query: &str,
    ) -> Option<String> {
        mysql_tree_explain_query_kind(&format!("EXPLAIN ANALYZE FORMAT=TREE {query}"))?;
        Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}"))
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
        let where_clause = pk_pairs
            .iter()
            .map(|(column, value)| mysql_equality_predicate(column, value))
            .collect::<Vec<_>>()
            .join(" AND ");

        format!(
            "UPDATE {}.{}\nSET {} = {}\nWHERE {};",
            mysql_quote_identifier(schema),
            mysql_quote_identifier(table),
            mysql_quote_identifier(column),
            mysql_sql_literal(new_value),
            where_clause
        )
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        schema: &str,
        table: &str,
        pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        assert!(
            !pk_pairs_per_row.is_empty(),
            "pk_pairs_per_row must not be empty"
        );

        let predicates = pk_pairs_per_row
            .iter()
            .map(|pairs| {
                pairs
                    .iter()
                    .map(|(column, value)| mysql_equality_predicate(column, value))
                    .collect::<Vec<_>>()
                    .join(" AND ")
            })
            .collect::<Vec<_>>();
        let where_clause = if predicates.len() == 1 {
            predicates[0].clone()
        } else {
            predicates
                .into_iter()
                .map(|predicate| format!("({predicate})"))
                .collect::<Vec<_>>()
                .join(" OR ")
        };

        format!(
            "DELETE FROM {}.{}\nWHERE {};",
            mysql_quote_identifier(schema),
            mysql_quote_identifier(table),
            where_clause
        )
    }
}

fn mysql_quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn mysql_sql_literal(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => "NULL".to_string(),
        QueryValue::Text(value) => mysql_quote_string(value),
        QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                let _ = write!(hex, "{byte:02X}");
            }
            format!("X'{hex}'")
        }
    }
}

fn mysql_quote_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\0' => escaped.push_str("\\0"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{001a}' => escaped.push_str("\\Z"),
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            _ => escaped.push(character),
        }
    }
    format!("'{escaped}'")
}

fn mysql_equality_predicate(column: &str, value: &QueryValue) -> String {
    let column = mysql_quote_identifier(column);
    match value {
        QueryValue::Null => format!("{column} IS NULL"),
        _ => format!("{column} = {}", mysql_sql_literal(value)),
    }
}

#[cfg(test)]
mod write_sql_tests {
    use super::*;

    #[test]
    fn update_quotes_identifiers_and_mysql_string_escapes() {
        let adapter = MySqlAdapter::new();
        let sql = adapter.build_update_sql(
            DatabaseType::MySQL,
            "db`name",
            "table`name",
            "value`name",
            &QueryValue::text("O'Reilly\\path\n\t\0"),
            &[
                (
                    "id`part".to_string(),
                    QueryValue::SqlLiteral("18446744073709551615".into()),
                ),
                ("tenant".to_string(), QueryValue::Null),
            ],
        );

        assert_eq!(
            sql,
            "UPDATE \x60db\x60\x60name\x60.\x60table\x60\x60name\x60\nSET \x60value\x60\x60name\x60 = 'O\\'Reilly\\\\path\\n\\t\\0'\nWHERE \x60id\x60\x60part\x60 = 18446744073709551615 AND \x60tenant\x60 IS NULL;"
        );
    }

    #[test]
    fn update_uses_text_datetime_and_blob_literals_without_coercion() {
        let adapter = MySqlAdapter::new();
        let sql = adapter.build_update_sql(
            DatabaseType::MySQL,
            "sabiql_test",
            "events",
            "payload",
            &QueryValue::Blob(vec![0, 255, 16]),
            &[(
                "created_at".to_string(),
                QueryValue::text("2026-08-13 12:34:56"),
            )],
        );

        assert_eq!(
            sql,
            "UPDATE `sabiql_test`.`events`\nSET `payload` = X'00FF10'\nWHERE `created_at` = '2026-08-13 12:34:56';"
        );
    }

    #[test]
    fn json_document_update_keeps_json_null_distinct_from_string_null() {
        let adapter = MySqlAdapter::new();
        let json_null = adapter.build_update_sql(
            DatabaseType::MySQL,
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text("null"),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );
        let string_null = adapter.build_update_sql(
            DatabaseType::MySQL,
            "sabiql_test",
            "documents",
            "payload",
            &QueryValue::text(r#""null""#),
            &[("id".to_string(), QueryValue::SqlLiteral("1".into()))],
        );

        assert!(json_null.contains("SET `payload` = 'null'"));
        assert!(string_null.contains("SET `payload` = '\"null\"'"));
        assert_ne!(json_null, string_null);
    }

    #[test]
    fn bulk_delete_targets_each_composite_primary_key_row() {
        let adapter = MySqlAdapter::new();
        let sql = adapter.build_bulk_delete_sql(
            DatabaseType::MySQL,
            "sabiql_test",
            "items",
            &[
                vec![
                    ("first".to_string(), QueryValue::SqlLiteral("1".into())),
                    ("second".to_string(), QueryValue::SqlLiteral("20".into())),
                ],
                vec![
                    ("first".to_string(), QueryValue::SqlLiteral("2".into())),
                    ("second".to_string(), QueryValue::SqlLiteral("10".into())),
                ],
            ],
        );

        assert_eq!(
            sql,
            "DELETE FROM `sabiql_test`.`items`\nWHERE (`first` = 1 AND `second` = 20) OR (`first` = 2 AND `second` = 10);"
        );
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

#[cfg(test)]
mod explain_tests {
    use super::*;

    #[test]
    fn builds_tree_explain_for_select_table_and_dml() {
        let adapter = MySqlAdapter::new();

        for query in [
            "SELECT * FROM users",
            "TABLE users",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "REPLACE users VALUES (1)",
            "REPLACE LOW_PRIORITY INTO users VALUES (1)",
            "REPLACE DELAYED users VALUES (1)",
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
        ] {
            assert_eq!(
                adapter.build_explain_sql(DatabaseType::MySQL, query),
                Some(format!("EXPLAIN FORMAT=TREE {query}")),
                "{query}"
            );
        }
    }

    #[test]
    fn rejects_mysql_explain_for_unsupported_input() {
        let adapter = MySqlAdapter::new();

        for query in [
            "CREATE TABLE users(id INT)",
            "DROP TABLE users",
            "\\C /tmp/other.sock",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(
                adapter.build_explain_sql(DatabaseType::MySQL, query),
                None,
                "{query}"
            );
        }
    }

    #[test]
    fn builds_tree_explain_analyze_only_for_side_effect_free_reads() {
        let adapter = MySqlAdapter::new();

        for query in ["SELECT * FROM users", "TABLE users"] {
            assert_eq!(
                adapter.build_explain_analyze_sql(DatabaseType::MySQL, query),
                Some(format!("EXPLAIN ANALYZE FORMAT=TREE {query}")),
                "{query}"
            );
        }

        for query in [
            "UPDATE users SET name = 'Ada' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
            "INSERT INTO users VALUES (1)",
            "REPLACE INTO users VALUES (1)",
            "SELECT * FROM users FOR UPDATE",
            "SELECT 1; SELECT 2",
        ] {
            assert_eq!(
                adapter.build_explain_analyze_sql(DatabaseType::MySQL, query),
                None,
                "{query}"
            );
        }
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

#[derive(Debug)]
struct MySqlDsn {
    host: String,
    port: u16,
    database: Option<String>,
    username: String,
    password: String,
    ssl_mode: MySqlSslMode,
    ssl_ca: Option<String>,
    ssl_cert: Option<String>,
    ssl_key: Option<String>,
}

fn build_mysql_dsn(config: &MySqlConnectionConfig) -> String {
    let mut url = Url::parse("mysql://localhost").expect("static MySQL URL is valid");
    url.set_username(&config.username)
        .expect("MySQL username is valid URL data");
    url.set_password(Some(&config.password))
        .expect("MySQL password is valid URL data");
    let host = normalize_mysql_host(&config.host);
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    };
    url.set_host(Some(&host))
        .expect("validated MySQL host is valid URL data");
    url.set_port(Some(config.port))
        .expect("MySQL port is valid URL data");
    if let Some(database) = config.database.as_deref() {
        url.path_segments_mut()
            .expect("MySQL URL supports path segments")
            .push(database);
    }
    url.query_pairs_mut()
        .append_pair("ssl-mode", &config.ssl_mode.to_string());
    if let Some(path) = config.ssl_ca.as_deref() {
        url.query_pairs_mut().append_pair("ssl-ca", path);
    }
    if let Some(path) = config.ssl_cert.as_deref() {
        url.query_pairs_mut().append_pair("ssl-cert", path);
    }
    if let Some(path) = config.ssl_key.as_deref() {
        url.query_pairs_mut().append_pair("ssl-key", path);
    }
    url.to_string()
}

fn parse_mysql_dsn(dsn: &str) -> Result<MySqlDsn, DbOperationError> {
    let url = Url::parse(dsn).map_err(|error| {
        DbOperationError::ConnectionFailed(format!("Invalid MySQL DSN: {error}"))
    })?;
    if url.scheme() != "mysql" {
        return Err(DbOperationError::ConnectionFailed(
            "Invalid MySQL DSN scheme".to_string(),
        ));
    }
    let host = url.host_str().ok_or_else(|| {
        DbOperationError::ConnectionFailed("MySQL DSN is missing a host".to_string())
    })?;
    let host = normalize_mysql_host(host);
    let username = decode_url_component(url.username())?;
    let password = decode_url_component(url.password().unwrap_or_default())?;
    let database = url
        .path_segments()
        .and_then(|mut segments| segments.next())
        .filter(|segment| !segment.is_empty())
        .map(decode_url_component)
        .transpose()?;
    let ssl_mode = url
        .query_pairs()
        .find_map(|(key, value)| (key == "ssl-mode").then(|| parse_ssl_mode(&value)))
        .transpose()?
        .unwrap_or_default();
    let ssl_ca = url
        .query_pairs()
        .find_map(|(key, value)| (key == "ssl-ca").then(|| value.into_owned()));
    let ssl_cert = url
        .query_pairs()
        .find_map(|(key, value)| (key == "ssl-cert").then(|| value.into_owned()));
    let ssl_key = url
        .query_pairs()
        .find_map(|(key, value)| (key == "ssl-key").then(|| value.into_owned()));

    Ok(MySqlDsn {
        host,
        port: url.port().unwrap_or(3306),
        database,
        username,
        password,
        ssl_mode,
        ssl_ca,
        ssl_cert,
        ssl_key,
    })
}

fn normalize_mysql_host(host: &str) -> String {
    host.strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host)
        .to_string()
}

fn parse_ssl_mode(value: &str) -> Result<MySqlSslMode, DbOperationError> {
    match value {
        "DISABLED" => Ok(MySqlSslMode::Disabled),
        "PREFERRED" => Ok(MySqlSslMode::Preferred),
        "REQUIRED" => Ok(MySqlSslMode::Required),
        "VERIFY_CA" => Ok(MySqlSslMode::VerifyCa),
        "VERIFY_IDENTITY" => Ok(MySqlSslMode::VerifyIdentity),
        _ => Err(DbOperationError::ConnectionFailed(
            "Invalid MySQL TLS mode".to_string(),
        )),
    }
}

fn decode_url_component(value: &str) -> Result<String, DbOperationError> {
    urlencoding::decode(value)
        .map(std::borrow::Cow::into_owned)
        .map_err(|error| DbOperationError::ConnectionFailed(format!("Invalid MySQL DSN: {error}")))
}

fn validate_mysql_values(target: &MySqlDsn) -> Result<(), DbOperationError> {
    let values = [
        Some(target.host.as_str()),
        Some(target.username.as_str()),
        Some(target.password.as_str()),
        target.database.as_deref(),
        target.ssl_ca.as_deref(),
        target.ssl_cert.as_deref(),
        target.ssl_key.as_deref(),
    ];
    if values
        .into_iter()
        .flatten()
        .any(|value| value.chars().any(char::is_control))
    {
        return Err(DbOperationError::ConnectionFailed(
            "MySQL connection settings contain a control character".to_string(),
        ));
    }
    Ok(())
}

fn validate_mysql_tls_files(target: &MySqlDsn) -> Result<(), DbOperationError> {
    let has_tls_path =
        target.ssl_ca.is_some() || target.ssl_cert.is_some() || target.ssl_key.is_some();
    if target.ssl_mode == MySqlSslMode::Disabled && has_tls_path {
        return Err(DbOperationError::ConnectionFailed(
            "MySQL TLS paths require an enabled TLS mode".to_string(),
        ));
    }
    if matches!(
        target.ssl_mode,
        MySqlSslMode::VerifyCa | MySqlSslMode::VerifyIdentity
    ) && target.ssl_ca.is_none()
    {
        return Err(DbOperationError::ConnectionFailed(
            "MySQL CA path is required for certificate verification".to_string(),
        ));
    }
    if target.ssl_cert.is_some() != target.ssl_key.is_some() {
        return Err(DbOperationError::ConnectionFailed(
            "MySQL client certificate and key must be specified together".to_string(),
        ));
    }

    for (kind, path) in [
        ("CA", target.ssl_ca.as_deref()),
        ("client certificate", target.ssl_cert.as_deref()),
        ("client key", target.ssl_key.as_deref()),
    ] {
        let Some(path) = path else { continue };
        let metadata = fs::metadata(path).map_err(|error| {
            DbOperationError::ConnectionFailed(format!(
                "MySQL {kind} path cannot be accessed: {error}"
            ))
        })?;
        if !metadata.is_file() {
            return Err(DbOperationError::ConnectionFailed(format!(
                "MySQL {kind} path is not a regular file"
            )));
        }
        let contents = fs::read(path).map_err(|error| {
            DbOperationError::ConnectionFailed(format!("MySQL {kind} cannot be read: {error}"))
        })?;
        let text = String::from_utf8_lossy(&contents);
        if matches!(kind, "CA" | "client certificate") && !text.contains("BEGIN CERTIFICATE") {
            return Err(DbOperationError::ConnectionFailed(format!(
                "MySQL {kind} is not a PEM certificate"
            )));
        }
        if kind == "client key" {
            if text.contains("BEGIN ENCRYPTED PRIVATE KEY")
                || text
                    .lines()
                    .any(|line| line.trim().eq_ignore_ascii_case("Proc-Type: 4,ENCRYPTED"))
            {
                return Err(DbOperationError::ConnectionFailed(
                    "Encrypted MySQL client keys are not supported".to_string(),
                ));
            }
            if ![
                "BEGIN PRIVATE KEY",
                "BEGIN RSA PRIVATE KEY",
                "BEGIN EC PRIVATE KEY",
            ]
            .iter()
            .any(|marker| text.contains(marker))
            {
                return Err(DbOperationError::ConnectionFailed(
                    "MySQL client key is not a PEM private key".to_string(),
                ));
            }
        }
    }
    Ok(())
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

fn mysql_probe_args(option_file: &std::path::Path) -> Vec<String> {
    vec![
        format!("--defaults-file={}", option_file.display()),
        "--no-login-paths".to_string(),
        "--protocol=TCP".to_string(),
        "--connect-timeout=10".to_string(),
        "--batch".to_string(),
        "--raw".to_string(),
        "--skip-column-names".to_string(),
        "--binary-mode".to_string(),
        "--skip-reconnect".to_string(),
        format!("--execute={MYSQL_PROBE_QUERY}"),
    ]
}

async fn run_mysql_command<I, S>(
    args: I,
    option_file: Option<&PathBuf>,
) -> Result<std::process::Output, DbOperationError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("mysql");
    command
        .args(args)
        .stdin(Stdio::null())
        .env_remove("MYSQL_PWD")
        .env_remove("MYSQL_PASSWORD")
        .kill_on_drop(true);
    if option_file.is_some() {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    match timeout(MYSQL_PROBE_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) if error.kind() == io::ErrorKind::NotFound => {
            Err(DbOperationError::CommandNotFound {
                command: DatabaseCli::MySql,
                details: error.to_string(),
            })
        }
        Ok(Err(error)) => Err(DbOperationError::ConnectionFailed(error.to_string())),
        Err(_) => Err(DbOperationError::Timeout(
            "mysql probe exceeded the connection timeout".to_string(),
        )),
    }
}

fn clean_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr).trim().to_string();
    if text.is_empty() {
        "mysql probe failed".to_string()
    } else {
        text
    }
}

fn classify_mysql_probe_failure(stderr: String) -> DbOperationError {
    if is_mysql_connect_timeout_message(&stderr) {
        DbOperationError::Timeout(stderr)
    } else {
        DbOperationError::ConnectionFailed(stderr)
    }
}

fn is_mysql_connect_timeout_message(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("can't connect to mysql server")
        && (lower.contains("(110)") || lower.contains("(10060)"))
}

struct MySqlOptionFile {
    path: PathBuf,
}

impl MySqlOptionFile {
    fn create(target: &MySqlDsn) -> Result<Self, DbOperationError> {
        validate_mysql_values(target)?;
        validate_mysql_tls_files(target)?;
        let mut path = std::env::temp_dir();
        path.push(format!("sabiql-mysql-{}.cnf", Uuid::new_v4()));
        if !path.is_absolute() {
            path = std::env::current_dir()
                .map_err(|error| DbOperationError::ConnectionFailed(error.to_string()))?
                .join(path);
        }

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&path).map_err(|error| {
            DbOperationError::ConnectionFailed(format!(
                "Unable to create MySQL option file: {error}"
            ))
        })?;
        let contents = serialize_option_file(target);
        if let Err(error) = file.write_all(contents.as_bytes()) {
            let _ = fs::remove_file(&path);
            return Err(DbOperationError::ConnectionFailed(format!(
                "Unable to write MySQL option file: {error}"
            )));
        }
        Ok(Self { path })
    }
}

impl Drop for MySqlOptionFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn serialize_option_file(target: &MySqlDsn) -> String {
    let mut contents = String::from("[client]\n");
    push_option(&mut contents, "host", &target.host);
    push_option(&mut contents, "port", &target.port.to_string());
    push_option(&mut contents, "user", &target.username);
    push_option(&mut contents, "password", &target.password);
    if let Some(database) = target.database.as_deref() {
        push_option(&mut contents, "database", database);
    }
    push_option(&mut contents, "ssl-mode", &target.ssl_mode.to_string());
    if let Some(path) = target.ssl_ca.as_deref() {
        push_option(&mut contents, "ssl-ca", path);
    }
    if let Some(path) = target.ssl_cert.as_deref() {
        push_option(&mut contents, "ssl-cert", path);
    }
    if let Some(path) = target.ssl_key.as_deref() {
        push_option(&mut contents, "ssl-key", path);
    }
    contents
}

fn push_option(contents: &mut String, key: &str, value: &str) {
    contents.push_str(key);
    contents.push_str(" = ");
    contents.push_str(&quote_option_value(value));
    contents.push('\n');
}

fn quote_option_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(character),
        }
    }
    format!("\"{escaped}\"")
}

#[derive(Debug, PartialEq, Eq)]
struct MysqlResultSet {
    columns: Vec<String>,
    values: Vec<Vec<QueryValue>>,
}

#[derive(Debug, PartialEq, Eq)]
struct MysqlExecutionResult {
    result_set: Option<MysqlResultSet>,
    command_tag: Option<CommandTag>,
    refresh_scope: RefreshScope,
}

const MYSQL_BATCH_MARKER_HEADER: &[u8] = b"__sabiql_marker\taffected_rows";

async fn execute_mysql_batch_statement(
    process: &mut MysqlProcess,
    query: &str,
    marker: &str,
) -> Result<(Option<MysqlResultSet>, MysqlResultSet), DbOperationError> {
    write_mysql_statement(process, query).await?;
    write_mysql_statement(
        process,
        &format!("SELECT '{marker}' AS __sabiql_marker, ROW_COUNT() AS affected_rows"),
    )
    .await?;
    let output = read_mysql_batch_until_marker(process, marker).await?;
    parse_mysql_batch_execution(&output, query, marker)
}

fn parse_mysql_batch_execution(
    output: &[u8],
    query: &str,
    marker: &str,
) -> Result<(Option<MysqlResultSet>, MysqlResultSet), DbOperationError> {
    if output
        .split(|byte| *byte == b'\n' || *byte == b'\r')
        .any(|line| line.starts_with(b"Field   1:"))
    {
        return parse_mysql_table_execution(output, marker);
    }
    let marker_query =
        format!("SELECT '{marker}' AS __sabiql_marker, ROW_COUNT() AS affected_rows");
    let lines = output
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !is_mysql_batch_diagnostic(line))
        .collect::<Vec<_>>();
    let marker_header = lines
        .iter()
        .position(|line| *line == MYSQL_BATCH_MARKER_HEADER)
        .ok_or_else(|| {
            if has_mysql_cli_error(output) {
                classify_mysql_query_failure(output)
            } else {
                DbOperationError::QueryFailed(
                    "MySQL batch result did not contain the row-count marker".to_string(),
                )
            }
        })?;
    let marker_line = lines
        .iter()
        .skip(marker_header + 1)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            DbOperationError::QueryFailed("MySQL batch row-count marker was incomplete".to_string())
        })?;
    let marker_values = parse_mysql_batch_record(marker_line)?;
    if marker_values.len() != 2
        || marker_values[0].as_str() != Some(marker)
        || marker_values[1].as_str().is_none()
    {
        return Err(DbOperationError::QueryFailed(
            "MySQL batch row-count marker did not match the executed statement".to_string(),
        ));
    }

    let user_start = lines[..marker_header]
        .iter()
        .position(|line| !line.is_empty() && !is_mysql_batch_query_echo(line, query, &marker_query))
        .unwrap_or(marker_header);
    let user_lines = lines[user_start..marker_header].to_vec();
    if has_mysql_cli_error(output) {
        let has_unexpected_error_line = user_lines.iter().any(|line| {
            if !line.starts_with(b"ERROR ") && *line != b"ERROR" {
                return false;
            }
            user_lines
                .first()
                .and_then(|header| parse_mysql_batch_header(header).ok())
                .is_none_or(|header| {
                    parse_mysql_batch_record(line)
                        .map_or(true, |values| values.len() != header.len())
                })
        });
        if has_unexpected_error_line
            || user_lines.is_empty()
            || user_lines
                .first()
                .is_some_and(|line| line.starts_with(b"ERROR ") || *line == b"ERROR")
        {
            return Err(classify_mysql_query_failure(output));
        }
    }
    let user_result = if user_lines.is_empty() {
        None
    } else {
        let columns = parse_mysql_batch_header(user_lines[0])?;
        let values = user_lines[1..]
            .iter()
            .map(|line| {
                let values = parse_mysql_batch_record(line)?;
                if values.len() != columns.len() {
                    return Err(DbOperationError::QueryFailed(
                        "MySQL batch rows have inconsistent fields".to_string(),
                    ));
                }
                Ok(values)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Some(MysqlResultSet { columns, values })
    };

    Ok((
        user_result,
        MysqlResultSet {
            columns: vec!["__sabiql_marker".to_string(), "affected_rows".to_string()],
            values: vec![marker_values],
        },
    ))
}

#[derive(Debug, Default, Clone, Copy)]
enum MysqlTableFieldKind {
    #[default]
    Other,
    Null,
    Numeric,
    Text,
    BinaryText,
}

#[derive(Debug, Default)]
struct MysqlTableField {
    name: String,
    start: usize,
    kind: MysqlTableFieldKind,
    not_null: bool,
    max_length: usize,
}

fn parse_mysql_table_execution(
    output: &[u8],
    marker: &str,
) -> Result<(Option<MysqlResultSet>, MysqlResultSet), DbOperationError> {
    let lines = output
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !is_mysql_batch_diagnostic(line))
        .collect::<Vec<_>>();
    let fields = parse_mysql_table_fields(&lines)?;
    let marker_index = fields
        .iter()
        .rposition(|field| field.name == "__sabiql_marker")
        .ok_or_else(|| {
            DbOperationError::QueryFailed(
                "MySQL table result did not contain the row-count marker".to_string(),
            )
        })?;
    let marker_fields = fields.get(marker_index..marker_index + 2).ok_or_else(|| {
        DbOperationError::QueryFailed("MySQL table row-count marker was incomplete".to_string())
    })?;
    if marker_fields[1].name != "affected_rows" {
        return Err(DbOperationError::QueryFailed(
            "MySQL table row-count marker did not match the executed statement".to_string(),
        ));
    }
    let marker_values = parse_mysql_table_rows(&lines[marker_fields[0].start..], marker_fields)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            DbOperationError::QueryFailed("MySQL table row-count marker was empty".to_string())
        })?;
    if marker_values.len() != 2 || marker_values[0].as_str() != Some(marker) {
        return Err(DbOperationError::QueryFailed(
            "MySQL table row-count marker did not match the executed statement".to_string(),
        ));
    }
    let marker_result = MysqlResultSet {
        columns: vec!["__sabiql_marker".to_string(), "affected_rows".to_string()],
        values: vec![marker_values],
    };
    let user_fields = &fields[..marker_index];
    let user_result = if user_fields.is_empty() {
        None
    } else {
        let values = parse_mysql_table_rows(
            &lines[user_fields[0].start..marker_fields[0].start],
            user_fields,
        )?;
        Some(MysqlResultSet {
            columns: user_fields.iter().map(|field| field.name.clone()).collect(),
            values,
        })
    };
    Ok((user_result, marker_result))
}

fn parse_mysql_table_fields(lines: &[&[u8]]) -> Result<Vec<MysqlTableField>, DbOperationError> {
    let mut fields = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with(b"Field ") {
            let name = line.split(|byte| *byte == b'`').nth(1).ok_or_else(|| {
                DbOperationError::QueryFailed(
                    "MySQL table field metadata had no column name".to_string(),
                )
            })?;
            let name = String::from_utf8(name.to_vec()).map_err(|error| {
                DbOperationError::QueryFailed(format!("invalid MySQL table column name: {error}"))
            })?;
            fields.push(MysqlTableField {
                name,
                start: index,
                ..MysqlTableField::default()
            });
            continue;
        }
        let Some(field) = fields.last_mut() else {
            continue;
        };
        if let Some(value) = line.strip_prefix(b"Type:") {
            let value = value.trim_ascii();
            field.kind = if value == b"NULL" {
                MysqlTableFieldKind::Null
            } else if matches!(
                value,
                b"TINY"
                    | b"SHORT"
                    | b"LONG"
                    | b"LONGLONG"
                    | b"INT24"
                    | b"DECIMAL"
                    | b"NEWDECIMAL"
                    | b"FLOAT"
                    | b"DOUBLE"
            ) {
                MysqlTableFieldKind::Numeric
            } else if matches!(value, b"VAR_STRING" | b"VARCHAR" | b"STRING" | b"JSON") {
                MysqlTableFieldKind::Text
            } else {
                MysqlTableFieldKind::Other
            };
        } else if let Some(value) = line.strip_prefix(b"Max_length:") {
            field.max_length = String::from_utf8_lossy(value)
                .trim()
                .parse()
                .unwrap_or_default();
        } else if let Some(value) = line.strip_prefix(b"Flags:") {
            field.not_null = value
                .split(u8::is_ascii_whitespace)
                .any(|flag| flag == b"NOT_NULL");
            let binary = value
                .split(u8::is_ascii_whitespace)
                .any(|flag| flag == b"BINARY");
            if binary && matches!(field.kind, MysqlTableFieldKind::Text) {
                field.kind = MysqlTableFieldKind::BinaryText;
            }
        }
    }
    Ok(fields)
}

fn parse_mysql_table_rows(
    lines: &[&[u8]],
    fields: &[MysqlTableField],
) -> Result<Vec<Vec<QueryValue>>, DbOperationError> {
    let border_indices = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            (line.starts_with(b"+") && line.ends_with(b"+")).then_some(index)
        })
        .collect::<Vec<_>>();
    if border_indices.len() < 2 {
        return Ok(Vec::new());
    }
    let data_start = border_indices[1] + 1;
    let data_end = border_indices.get(2).copied().unwrap_or(lines.len());
    let mut rows = Vec::new();
    let mut current: Option<Vec<String>> = None;
    for line in &lines[data_start..data_end] {
        if line.starts_with(b"|") {
            if let Some(row) = current.take()
                && row.len() == fields.len()
            {
                rows.push(row);
            }
            let ends_with_pipe = line.ends_with(b"|");
            let cells = line.strip_prefix(b"|").unwrap_or(line);
            let cells = cells.strip_suffix(b"|").unwrap_or(cells);
            let cells = cells
                .split(|byte| *byte == b'|')
                .enumerate()
                .map(|(index, value)| {
                    fields.get(index).map_or_else(
                        || {
                            Err(DbOperationError::QueryFailed(format!(
                                "MySQL table row has too many fields: {}",
                                String::from_utf8_lossy(line)
                            )))
                        },
                        |field| {
                            Ok(table_cell_text_with_alignment(
                                value,
                                matches!(field.kind, MysqlTableFieldKind::Numeric),
                            ))
                        },
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            current = Some(cells);
            if ends_with_pipe
                && let Some(row) = current.take()
                && row.len() == fields.len()
            {
                rows.push(row);
            }
        } else if let Some(row) = current.as_mut() {
            if let Some(last) = row.last_mut() {
                let value =
                    table_cell_text_with_alignment(line.strip_suffix(b"|").unwrap_or(line), false);
                if !last.is_empty() {
                    last.push('\n');
                }
                last.push_str(&value);
            }
            if line.ends_with(b"|") && row.len() == fields.len() {
                rows.push(std::mem::take(row));
                current = None;
            }
        }
    }
    if let Some(row) = current
        && row.len() == fields.len()
    {
        rows.push(row);
    }
    rows.into_iter()
        .map(|row| {
            let values = row
                .into_iter()
                .zip(fields)
                .map(|(value, field)| {
                    if value == "NULL"
                        && (!field.not_null
                            && (!matches!(field.kind, MysqlTableFieldKind::BinaryText)
                                || field.max_length == 0)
                            || matches!(field.kind, MysqlTableFieldKind::Null))
                    {
                        QueryValue::Null
                    } else {
                        QueryValue::Text(value)
                    }
                })
                .collect::<Vec<_>>();
            Ok(values)
        })
        .collect()
}

fn table_cell_text(value: &[u8]) -> String {
    table_cell_text_with_alignment(value, false)
}

fn table_cell_text_with_alignment(value: &[u8], right_aligned: bool) -> String {
    let value = String::from_utf8_lossy(value);
    if right_aligned {
        value.trim().to_string()
    } else {
        value
            .strip_prefix(' ')
            .unwrap_or(&value)
            .trim_end()
            .to_string()
    }
}

fn is_mysql_batch_diagnostic(line: &[u8]) -> bool {
    line.starts_with(b"mysql: ") || line.starts_with(b"Warning: ")
}

fn is_mysql_batch_query_echo(line: &[u8], query: &str, marker_query: &str) -> bool {
    [query, marker_query].iter().any(|statement| {
        let statement = statement.trim_end();
        statement.split('\n').enumerate().any(|(index, part)| {
            let part = part.trim_end_matches('\r');
            line == part.as_bytes()
                || (index == statement.split('\n').count() - 1
                    && line == format!("{part};").as_bytes())
        })
    })
}

fn parse_mysql_batch_header(line: &[u8]) -> Result<Vec<String>, DbOperationError> {
    line.split(|byte| *byte == b'\t')
        .map(|field| {
            String::from_utf8(field.to_vec()).map_err(|error| {
                DbOperationError::QueryFailed(format!("invalid MySQL batch column name: {error}"))
            })
        })
        .collect()
}

fn parse_mysql_batch_record(line: &[u8]) -> Result<Vec<QueryValue>, DbOperationError> {
    line.split(|byte| *byte == b'\t')
        .map(parse_mysql_batch_value)
        .collect()
}

fn parse_mysql_batch_value(value: &[u8]) -> Result<QueryValue, DbOperationError> {
    if value == b"NULL" {
        return Ok(QueryValue::Null);
    }
    let mut decoded = Vec::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        if value[index] != b'\\' {
            decoded.push(value[index]);
            index += 1;
            continue;
        }
        index += 1;
        let escaped = value.get(index).copied().ok_or_else(|| {
            DbOperationError::QueryFailed("invalid MySQL batch escape".to_string())
        })?;
        decoded.push(match escaped {
            b'0' => 0,
            b'b' => 8,
            b't' => b'\t',
            b'n' => b'\n',
            b'r' => b'\r',
            b'Z' => 26,
            b'\\' => b'\\',
            other => other,
        });
        index += 1;
    }
    String::from_utf8(decoded)
        .map(QueryValue::Text)
        .map_err(|error| {
            DbOperationError::QueryFailed(format!("invalid MySQL batch value: {error}"))
        })
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

struct MysqlProcess {
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
}

#[cfg(unix)]
struct MysqlPty {
    input: TokioFile,
    output: TokioFile,
    pending: Vec<u8>,
}

impl MysqlProcess {
    fn spawn_with_program(
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
            },
        })
    }
}

struct MysqlMetadataSession {
    process: MysqlProcess,
}

impl MysqlMetadataSession {
    fn spawn_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        Ok(Self {
            process: MysqlProcess::spawn_with_program(program, option_file)?,
        })
    }

    async fn probe(&mut self) -> Result<(), DbOperationError> {
        let marker = Uuid::new_v4().simple().to_string();
        let query =
            format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
        let result = self.execute(&query).await?;
        validate_mode_probe(&result, &marker)
    }

    async fn execute(&mut self, query: &str) -> Result<MysqlResultSet, DbOperationError> {
        let marker = Uuid::new_v4().simple().to_string();
        let (result, _) = execute_mysql_batch_statement(&mut self.process, query, &marker).await?;
        result.ok_or_else(|| {
            DbOperationError::MetadataParseFailed(
                "MySQL metadata query returned no result set".to_string(),
            )
        })
    }

    async fn finish(&mut self) -> Result<(), DbOperationError> {
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

    async fn cleanup(&mut self) {
        cleanup_mysql_process(&mut self.process).await;
    }
}

#[cfg(unix)]
fn create_mysql_pty() -> io::Result<(std::fs::File, std::fs::File)> {
    let mut master = -1;
    let mut slave = -1;
    let result = unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }

    let slave_file = unsafe { std::fs::File::from_raw_fd(slave) };
    let master_file = unsafe { std::fs::File::from_raw_fd(master) };
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(slave_file.as_raw_fd(), termios.as_mut_ptr()) } == 0 {
        let mut termios = unsafe { termios.assume_init() };
        termios.c_lflag &= !(libc::ECHO | libc::ECHONL);
        termios.c_oflag &= !libc::OPOST;
        let _ =
            unsafe { libc::tcsetattr(slave_file.as_raw_fd(), libc::TCSANOW, &raw const termios) };
    }
    Ok((master_file, slave_file))
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

async fn export_mysql_csv_to_file(
    target: MySqlDsn,
    query: &str,
    path: PathBuf,
) -> Result<(), DbOperationError> {
    let option_file = MySqlOptionFile::create(&target)?;
    let mut process = MysqlProcess::spawn_with_program(OsStr::new("mysql"), &option_file.path)?;
    let result = timeout(
        MYSQL_EXPORT_TIMEOUT,
        run_mysql_export_process(&mut process, query, path),
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
    query: &str,
    path: PathBuf,
) -> Result<(), DbOperationError> {
    let marker = Uuid::new_v4().simple().to_string();
    let probe_query =
        format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
    let probe_result_marker = Uuid::new_v4().simple().to_string();
    let (probe, _) =
        execute_mysql_batch_statement(process, &probe_query, &probe_result_marker).await?;
    let probe = probe.ok_or_else(|| {
        DbOperationError::QueryFailed("mysql sql_mode probe returned no result".to_string())
    })?;
    validate_mode_probe(&probe, &marker)?;
    configure_mysql_session(process, AccessMode::ReadOnly).await?;

    write_mysql_statement(process, query).await?;
    let result_marker = Uuid::new_v4().simple().to_string();
    write_mysql_statement(
        process,
        &format!("SELECT '{result_marker}' AS __sabiql_marker, ROW_COUNT() AS affected_rows"),
    )
    .await?;
    let mut csv_writer = CsvFileWriter::create(path).await?;
    stream_mysql_batch_result_to_csv(process, &mut csv_writer, query, &result_marker).await?;

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
    if !status.success() {
        return Err(classify_mysql_query_failure(error_bytes));
    }

    csv_writer.finish().await
}

async fn stream_mysql_batch_result_to_csv(
    process: &mut MysqlProcess,
    csv_writer: &mut CsvFileWriter,
    query: &str,
    marker: &str,
) -> Result<(), DbOperationError> {
    let output = read_mysql_batch_until_marker(process, marker).await?;
    let (result, marker_result) = parse_mysql_batch_execution(&output, query, marker)?;
    mysql_row_count_marker(&marker_result, marker)?;
    let result = result.ok_or_else(|| {
        DbOperationError::QueryFailed("MySQL CSV query returned no result set".to_string())
    })?;
    csv_writer.write_record(result.columns.iter()).await?;
    for row in result.values {
        csv_writer
            .write_record(row.iter().map(batch_csv_value))
            .await?;
    }
    Ok(())
}

fn batch_csv_value(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => String::new(),
        QueryValue::Text(value) | QueryValue::SqlLiteral(value) => value.clone(),
        QueryValue::Blob(value) => {
            let mut hex = String::with_capacity(value.len() * 2);
            for byte in value {
                write!(hex, "{byte:02X}").expect("writing to a String cannot fail");
            }
            hex
        }
    }
}

#[cfg(test)]
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
        run_mysql_export_process(&mut process, query, path),
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

#[cfg(test)]
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

async fn run_mysql_single_statement(
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
    let probe_result_marker = Uuid::new_v4().simple().to_string();
    let (probe, _) =
        execute_mysql_batch_statement(process, &probe_query, &probe_result_marker).await?;
    let probe = probe.ok_or_else(|| {
        DbOperationError::QueryFailed("mysql sql_mode probe returned no result".to_string())
    })?;
    validate_mode_probe(&probe, &marker)?;
    configure_mysql_session(process, access_mode).await?;

    let result_marker = Uuid::new_v4().simple().to_string();
    let (result, _) = execute_mysql_batch_statement(process, query, &result_marker).await?;
    let result = result.ok_or_else(|| {
        DbOperationError::QueryFailed("mysql query returned no result set".to_string())
    })?;

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
    process
        .stdin
        .shutdown()
        .await
        .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
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
    if !status.success() {
        return Err(classify_mysql_query_failure(error_bytes));
    }
    Ok(result)
}

async fn run_mysql_adhoc_with_program_and_statements(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
    execution_timeout: Duration,
) -> Result<MysqlExecutionResult, DbOperationError> {
    let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
    let result = timeout(
        execution_timeout,
        run_mysql_adhoc_process(&mut process, query, statements, access_mode),
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
    _query: &str,
    statements: &[MysqlStatement],
    access_mode: AccessMode,
) -> Result<MysqlExecutionResult, DbOperationError> {
    let probe_marker = Uuid::new_v4().simple().to_string();
    let probe_query = format!(
        "SELECT '{probe_marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode"
    );
    let probe_result_marker = Uuid::new_v4().simple().to_string();
    let (probe, _) =
        execute_mysql_batch_statement(process, &probe_query, &probe_result_marker).await?;
    let probe = probe.ok_or_else(|| {
        DbOperationError::QueryFailed("mysql sql_mode probe returned no result".to_string())
    })?;
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
        let (user_result, marker_result) =
            match execute_mysql_batch_statement(process, &statement.sql, &marker).await {
                Ok(result) => result,
                Err(error) => {
                    return Err(if is_mysql_batch_marker_error(&error) {
                        query_failed_after_change(error, possible_refresh_scope)
                    } else {
                        query_failed_after_mysql_statement(
                            error,
                            refresh_scope,
                            possible_refresh_scope,
                        )
                    });
                }
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

fn is_mysql_batch_marker_error(error: &DbOperationError) -> bool {
    matches!(
        error,
        DbOperationError::QueryFailed(details)
            if details.contains("row-count marker") || details.contains("result did not contain")
    )
}

async fn configure_mysql_session(
    process: &mut MysqlProcess,
    access_mode: AccessMode,
) -> Result<(), DbOperationError> {
    if !access_mode.is_read_only() {
        return Ok(());
    }

    let set_marker = Uuid::new_v4().simple().to_string();
    let (_set_result, _) =
        execute_mysql_batch_statement(process, MYSQL_READ_ONLY_STATEMENT, &set_marker).await?;

    let marker = Uuid::new_v4().simple().to_string();
    write_mysql_statement(
        process,
        &format!("SELECT '{marker}' AS {MYSQL_SESSION_MARKER_COLUMN}"),
    )
    .await?;
    let marker_query_marker = Uuid::new_v4().simple().to_string();
    write_mysql_statement(
        process,
        &format!("SELECT '{marker_query_marker}' AS __sabiql_marker, ROW_COUNT() AS affected_rows"),
    )
    .await?;
    let output = read_mysql_batch_until_marker(process, &marker_query_marker).await?;
    let read_only_query = format!("SELECT '{marker}' AS {MYSQL_SESSION_MARKER_COLUMN}");
    let (result, _) = parse_mysql_batch_execution(&output, &read_only_query, &marker_query_marker)?;
    let result = result.ok_or_else(|| {
        DbOperationError::QueryFailed(
            "mysql read-only session marker returned no result".to_string(),
        )
    })?;
    validate_mysql_session_marker(&result, &marker)
}

fn validate_mysql_session_marker(
    result: &MysqlResultSet,
    marker: &str,
) -> Result<(), DbOperationError> {
    if result.columns != [MYSQL_SESSION_MARKER_COLUMN]
        || result.values.len() != 1
        || result.values[0].len() != 1
        || result.values[0][0].as_str() != Some(marker)
    {
        return Err(DbOperationError::QueryFailed(
            "mysql read-only session marker did not match".to_string(),
        ));
    }
    Ok(())
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

#[cfg(unix)]
async fn read_pty_all(pty: &mut MysqlPty) -> io::Result<Vec<u8>> {
    let mut output = std::mem::take(&mut pty.pending);
    let mut chunk = [0; 4096];
    loop {
        match pty.output.read(&mut chunk).await {
            Ok(0) => return Ok(output),
            Ok(count) => output.extend_from_slice(&chunk[..count]),
            Err(error) if matches!(error.raw_os_error(), Some(libc::EIO | libc::EPERM)) => {
                return Ok(output);
            }
            Err(error) => return Err(error),
        }
    }
}

fn has_mysql_cli_error(output: &[u8]) -> bool {
    output
        .split(|byte| *byte == b'\n' || *byte == b'\r')
        .any(|line| {
            let mut line = line;
            while line
                .first()
                .is_some_and(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
            {
                line = &line[1..];
            }
            line.starts_with(b"ERROR ") || line == b"ERROR"
        })
}

#[cfg(unix)]
fn trace_mysql_frame(kind: &str, bytes: usize) {
    if std::env::var_os("SABIQL_MYSQL_TRANSCRIPT").is_some() {
        write_mysql_transcript_line(&format!("sabiql mysql frame: {kind}, bytes={bytes}"));
    }
}

#[cfg(unix)]
fn trace_mysql_error(output: &[u8]) {
    if std::env::var_os("SABIQL_MYSQL_TRANSCRIPT").is_some() && has_mysql_cli_error(output) {
        write_mysql_transcript_line("sabiql mysql frame: ERROR line observed");
    }
}

#[cfg(unix)]
fn write_mysql_transcript_line(line: &str) {
    let mut stderr = io::stderr();
    let _ = stderr.write_all(line.as_bytes());
    let _ = stderr.write_all(b"\n");
}

async fn read_mysql_batch_until_marker(
    process: &mut MysqlProcess,
    marker: &str,
) -> Result<Vec<u8>, DbOperationError> {
    let mut output = Vec::new();
    loop {
        let line = match read_mysql_batch_line(process).await {
            Ok(line) => line,
            Err(_error) if has_mysql_cli_error(&output) => {
                return Err(classify_mysql_query_failure(&output));
            }
            Err(_error) if !output.is_empty() => {
                return Err(DbOperationError::QueryFailed(
                    "MySQL batch result ended before the row-count marker".to_string(),
                ));
            }
            Err(error) => return Err(error),
        };
        output.extend_from_slice(&line);
        if !line.ends_with(b"\n") {
            output.push(b'\n');
        }
        if mysql_batch_line_is_marker(&line, marker)? {
            return Ok(output);
        }
    }
}

fn mysql_batch_line_is_marker(line: &[u8], marker: &str) -> Result<bool, DbOperationError> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.starts_with(b"|") && line.ends_with(b"|") {
        return Ok(line
            .strip_prefix(b"|")
            .and_then(|line| line.strip_suffix(b"|"))
            .and_then(|line| line.split(|byte| *byte == b'|').next())
            .map(table_cell_text)
            .is_some_and(|value| value == marker));
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    let Some(first) = fields.next() else {
        return Ok(false);
    };
    let Some(second) = fields.next() else {
        return Ok(false);
    };
    if fields.next().is_some() {
        return Ok(false);
    }
    let first = parse_mysql_batch_value(first)?;
    let second = parse_mysql_batch_value(second)?;
    Ok(first.as_str() == Some(marker) && second.as_str().is_some())
}

#[cfg(unix)]
async fn read_mysql_batch_line(process: &mut MysqlProcess) -> Result<Vec<u8>, DbOperationError> {
    loop {
        if let Some(line_end) = process.pty.pending.iter().position(|byte| *byte == b'\n') {
            return Ok(process.pty.pending.drain(..=line_end).collect());
        }
        let mut chunk = [0; 4096];
        let count = match process.pty.output.read(&mut chunk).await {
            Ok(count) => count,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => 0,
            Err(error) => return Err(DbOperationError::ConnectionLost(error.to_string())),
        };
        if count == 0 {
            if process.pty.pending.is_empty() {
                return Err(DbOperationError::EmptyResponse(
                    "mysql query returned no batch result".to_string(),
                ));
            }
            return Ok(std::mem::take(&mut process.pty.pending));
        }
        process.pty.pending.extend_from_slice(&chunk[..count]);
    }
}

#[cfg(not(unix))]
async fn read_mysql_batch_line(process: &mut MysqlProcess) -> Result<Vec<u8>, DbOperationError> {
    let mut chunk = [0; 4096];
    let mut stderr_chunk = [0; 4096];
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    loop {
        if let Some(line_end) = process.pending.iter().position(|byte| *byte == b'\n') {
            return Ok(process.pending.drain(..=line_end).collect());
        }
        if stdout_closed {
            if process.pending.is_empty() {
                return Err(DbOperationError::EmptyResponse(
                    "mysql query returned no batch result".to_string(),
                ));
            }
            return Ok(std::mem::take(&mut process.pending));
        }
        if stderr_closed {
            let count = process
                .stdout
                .read(&mut chunk)
                .await
                .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
            if count == 0 {
                if process.pending.is_empty() {
                    return Err(DbOperationError::EmptyResponse(
                        "mysql query returned no batch result".to_string(),
                    ));
                }
                return Ok(std::mem::take(&mut process.pending));
            }
            process.pending.extend_from_slice(&chunk[..count]);
            continue;
        }
        tokio::select! {
            result = process.stdout.read(&mut chunk) => {
                let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                if count == 0 {
                    stdout_closed = true;
                } else {
                    process.pending.extend_from_slice(&chunk[..count]);
                }
            }
            result = process.stderr.read(&mut stderr_chunk) => {
                let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                if count == 0 {
                    stderr_closed = true;
                } else {
                    process.pending_stderr.extend_from_slice(&stderr_chunk[..count]);
                }
            }
        }
    }
}

#[cfg(not(unix))]
async fn read_all<R>(reader: &mut R) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    reader.read_to_end(&mut output).await?;
    Ok(output)
}

fn mysql_query_args(option_file: &std::path::Path) -> Vec<String> {
    vec![
        format!("--defaults-file={}", option_file.display()),
        "--no-login-paths".to_string(),
        "--protocol=TCP".to_string(),
        "--connect-timeout=10".to_string(),
        "--batch".to_string(),
        "--table".to_string(),
        "--column-names".to_string(),
        "--column-type-info".to_string(),
        "--binary-as-hex".to_string(),
        "--binary-mode".to_string(),
        "--unbuffered".to_string(),
        "--skip-reconnect".to_string(),
        "--default-character-set=utf8mb4".to_string(),
        "--prompt=".to_string(),
    ]
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

fn classify_mysql_query_failure(stderr: &[u8]) -> DbOperationError {
    let details = clean_mysql_stderr(stderr, "mysql query failed");
    let lower = details.to_ascii_lowercase();
    let error_code = mysql_server_error_code(&lower);
    if is_mysql_tls_error(&lower) {
        DbOperationError::ConnectionFailed(details)
    } else if is_mysql_connect_timeout_message(&details)
        || lower.contains("connect timeout")
        || lower.contains("connection timed out")
    {
        DbOperationError::Timeout(details)
    } else if matches!(error_code, Some(1044 | 1142 | 1143 | 1227))
        || lower.contains("command denied")
    {
        DbOperationError::PermissionDenied(details)
    } else if error_code == Some(1045)
        || lower.contains("access denied")
        || lower.contains("authentication")
    {
        DbOperationError::ConnectionFailed(details)
    } else if lower.contains("lost connection") || lower.contains("server has gone away") {
        DbOperationError::ConnectionLost(details)
    } else if lower.contains("lock wait timeout") || lower.contains("deadlock found") {
        DbOperationError::LockTimeout(details)
    } else if matches!(error_code, Some(1215 | 1216 | 1217 | 1451 | 1452))
        || lower.contains("foreign key constraint")
    {
        DbOperationError::ForeignKeyViolation(details)
    } else if lower.contains("doesn't exist") || lower.contains("does not exist") {
        DbOperationError::ObjectMissing(details)
    } else if lower.contains("duplicate entry") {
        DbOperationError::UniqueViolation(details)
    } else if lower.contains("query execution was interrupted")
        || lower.contains("query was interrupted")
    {
        DbOperationError::Canceled(details)
    } else {
        DbOperationError::QueryFailed(details)
    }
}

fn is_mysql_tls_error(lowercase_details: &str) -> bool {
    [
        "error 2026",
        "tls/ssl error",
        "ssl connection error",
        "ssl handshake",
        "tls handshake",
        "handshake failure",
        "tlsv1 alert",
        "certificate verify failed",
        "certificate verification failure",
        "certificate validation failure",
        "unable to get local issuer",
        "self-signed certificate",
        "unknown ca",
        "certificate required",
        "peer did not return a certificate",
    ]
    .iter()
    .any(|marker| lowercase_details.contains(marker))
}

fn mysql_server_error_code(lowercase_details: &str) -> Option<u32> {
    let marker = "error ";
    let start = lowercase_details.find(marker)? + marker.len();
    let digits = &lowercase_details[start..];
    let end = digits
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse().ok()
}

fn clean_mysql_stderr(stderr: &[u8], fallback: &str) -> String {
    let text = String::from_utf8_lossy(stderr).trim().to_string();
    if text.is_empty() {
        fallback.to_string()
    } else {
        text
    }
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum MysqlToken {
    Word(String),
    OpenParen,
    CloseParen,
    Comma,
    Other,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MysqlReadStatement {
    Select,
    Table,
    Show,
    Describe,
}

#[cfg(test)]
fn validate_mysql_adhoc_query(query: &str) -> Result<(), DbOperationError> {
    let tokens = scan_mysql_sql(query)?;
    let first = tokens.first().ok_or_else(|| {
        DbOperationError::UnsupportedOperation("empty MySQL statement".to_string())
    })?;
    let kind = match first {
        MysqlToken::Word(word) => match word.as_str() {
            "SELECT" => Some(MysqlReadStatement::Select),
            "TABLE" => Some(MysqlReadStatement::Table),
            "SHOW" => Some(MysqlReadStatement::Show),
            "DESCRIBE" => Some(MysqlReadStatement::Describe),
            "WITH" => classify_with_statement(&tokens),
            _ => None,
        },
        _ => None,
    };
    if kind.is_some() && has_top_level_into_clause(&tokens) {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL SELECT INTO clauses are not supported".to_string(),
        ));
    }
    kind.map(|_| ()).ok_or_else(|| {
        DbOperationError::UnsupportedOperation(
            "only a single SELECT, TABLE, SHOW, DESCRIBE, or SELECT CTE is supported".to_string(),
        )
    })
}

#[cfg(test)]
fn has_top_level_into_clause(tokens: &[MysqlToken]) -> bool {
    let mut depth = 0usize;
    for token in tokens {
        match token {
            MysqlToken::OpenParen => depth += 1,
            MysqlToken::CloseParen => depth = depth.saturating_sub(1),
            MysqlToken::Word(word) if depth == 0 && word == "INTO" => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
fn scan_mysql_sql(query: &str) -> Result<Vec<MysqlToken>, DbOperationError> {
    let chars: Vec<char> = query.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line_start = true;
    let mut semicolons = 0;
    let mut statement_ended = false;
    while index < chars.len() {
        let character = chars[index];
        if line_start && matches!(character, ' ' | '\t' | '\r') {
            index += 1;
            continue;
        }
        if line_start && character == '\\' {
            return Err(unsupported_client_command("backslash command"));
        }
        if line_start && character.is_ascii_alphabetic() {
            let start = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            if matches!(
                word.to_ascii_lowercase().as_str(),
                "delimiter" | "charset" | "source" | "system"
            ) && (index == chars.len() || chars[index].is_whitespace())
            {
                return Err(unsupported_client_command(&word));
            }
            if statement_ended {
                return Err(unsupported_client_command("multiple SQL statements"));
            }
            tokens.push(MysqlToken::Word(word.to_ascii_uppercase()));
            line_start = false;
            continue;
        }
        if character == '\n' {
            line_start = true;
            index += 1;
            continue;
        }
        if character.is_whitespace() {
            index += 1;
            continue;
        }
        if character == '#' {
            skip_line_comment(&chars, &mut index, &mut line_start);
            continue;
        }
        if character == '-'
            && chars.get(index + 1) == Some(&'-')
            && chars.get(index + 2).is_none_or(|next| next.is_whitespace())
        {
            skip_line_comment(&chars, &mut index, &mut line_start);
            continue;
        }
        if character == '/' && chars.get(index + 1) == Some(&'*') {
            if chars.get(index + 2) == Some(&'!') {
                return Err(unsupported_client_command("MySQL version comment"));
            }
            skip_block_comment(&chars, &mut index, &mut line_start)?;
            continue;
        }
        if matches!(character, '\'' | '"' | '`') {
            if statement_ended {
                return Err(unsupported_client_command("multiple SQL statements"));
            }
            skip_quoted_literal(&chars, &mut index, character)?;
            line_start = false;
            tokens.push(MysqlToken::Other);
            continue;
        }
        match character {
            ';' => {
                semicolons += 1;
                if semicolons > 1 {
                    return Err(unsupported_client_command("multiple SQL statements"));
                }
                if tokens.is_empty() {
                    return Err(unsupported_client_command("empty MySQL statement"));
                }
                statement_ended = true;
                index += 1;
            }
            '(' => {
                if statement_ended {
                    return Err(unsupported_client_command("multiple SQL statements"));
                }
                tokens.push(MysqlToken::OpenParen);
                line_start = false;
                index += 1;
            }
            ')' => {
                if statement_ended {
                    return Err(unsupported_client_command("multiple SQL statements"));
                }
                tokens.push(MysqlToken::CloseParen);
                line_start = false;
                index += 1;
            }
            ',' => {
                if statement_ended {
                    return Err(unsupported_client_command("multiple SQL statements"));
                }
                tokens.push(MysqlToken::Comma);
                line_start = false;
                index += 1;
            }
            character if character.is_ascii_alphabetic() || character == '_' => {
                if statement_ended {
                    return Err(unsupported_client_command("multiple SQL statements"));
                }
                let start = index;
                while index < chars.len()
                    && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '$'))
                {
                    index += 1;
                }
                let word: String = chars[start..index].iter().collect();
                tokens.push(MysqlToken::Word(word.to_ascii_uppercase()));
                line_start = false;
            }
            _ => {
                if statement_ended {
                    return Err(unsupported_client_command("multiple SQL statements"));
                }
                tokens.push(MysqlToken::Other);
                line_start = false;
                index += 1;
            }
        }
    }
    if tokens.is_empty() || query.trim_start().starts_with(';') {
        return Err(unsupported_client_command("empty MySQL statement"));
    }
    Ok(tokens)
}

#[cfg(test)]
fn skip_line_comment(chars: &[char], index: &mut usize, line_start: &mut bool) {
    while *index < chars.len() {
        let character = chars[*index];
        *index += 1;
        if character == '\n' {
            *line_start = true;
            break;
        }
    }
}

#[cfg(test)]
fn skip_block_comment(
    chars: &[char],
    index: &mut usize,
    line_start: &mut bool,
) -> Result<(), DbOperationError> {
    *index += 2;
    while *index < chars.len() {
        if chars[*index] == '\n' {
            *line_start = true;
        }
        if chars[*index] == '*' && chars.get(*index + 1) == Some(&'/') {
            *index += 2;
            return Ok(());
        }
        *index += 1;
    }
    Err(unsupported_client_command("unterminated block comment"))
}

#[cfg(test)]
fn skip_quoted_literal(
    chars: &[char],
    index: &mut usize,
    quote: char,
) -> Result<(), DbOperationError> {
    *index += 1;
    while *index < chars.len() {
        if chars[*index] == '\\' {
            *index += 2;
            continue;
        }
        if chars[*index] == quote {
            if chars.get(*index + 1) == Some(&quote) {
                *index += 2;
            } else {
                *index += 1;
                return Ok(());
            }
        } else {
            *index += 1;
        }
    }
    Err(unsupported_client_command("unterminated quoted literal"))
}

#[cfg(test)]
fn classify_with_statement(tokens: &[MysqlToken]) -> Option<MysqlReadStatement> {
    let mut index = 1;
    if matches!(tokens.get(index), Some(MysqlToken::Word(word)) if word == "RECURSIVE") {
        index += 1;
    }
    loop {
        if !matches!(
            tokens.get(index),
            Some(MysqlToken::Word(_) | MysqlToken::Other)
        ) {
            return None;
        }
        index += 1;
        if matches!(tokens.get(index), Some(MysqlToken::OpenParen)) {
            index = skip_parenthesized(tokens, index)?;
        }
        if !matches!(tokens.get(index), Some(MysqlToken::Word(word)) if word == "AS") {
            return None;
        }
        index += 1;
        if !matches!(tokens.get(index), Some(MysqlToken::OpenParen)) {
            return None;
        }
        index = skip_parenthesized(tokens, index)?;
        if matches!(tokens.get(index), Some(MysqlToken::Comma)) {
            index += 1;
            continue;
        }
        return match tokens.get(index) {
            Some(MysqlToken::Word(word)) if word == "SELECT" => Some(MysqlReadStatement::Select),
            _ => None,
        };
    }
}

#[cfg(test)]
fn skip_parenthesized(tokens: &[MysqlToken], mut index: usize) -> Option<usize> {
    let mut depth = 0;
    while let Some(token) = tokens.get(index) {
        match token {
            MysqlToken::OpenParen => depth += 1,
            MysqlToken::CloseParen => {
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

#[cfg(test)]
fn unsupported_client_command(command: &str) -> DbOperationError {
    DbOperationError::UnsupportedOperation(format!("unsupported MySQL {command}"))
}

#[cfg(test)]
mod probe_tests {
    use sabiql_app::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};

    use super::*;

    #[test]
    fn builds_and_parses_mysql_dsn_with_encoded_components() {
        let config = MySqlConnectionConfig::new(
            "db.example",
            3307,
            Some("app/schema".to_string()),
            "user name",
            "p@ss#word",
            MySqlSslMode::Required,
        );
        let dsn = build_mysql_dsn(&config);
        let parsed = parse_mysql_dsn(&dsn).unwrap();

        assert_eq!(parsed.host, "db.example");
        assert_eq!(parsed.port, 3307);
        assert_eq!(parsed.database.as_deref(), Some("app/schema"));
        assert_eq!(parsed.username, "user name");
        assert_eq!(parsed.password, "p@ss#word");
        assert_eq!(parsed.ssl_mode, MySqlSslMode::Required);
    }

    #[test]
    fn builds_and_parses_mysql_dsn_with_tls_paths() {
        let config = MySqlConnectionConfig::new(
            "db.example",
            3307,
            Some("app".to_string()),
            "user",
            "password",
            MySqlSslMode::VerifyIdentity,
        )
        .with_tls_paths(
            Some(r"C:\certs\ca #1.pem".to_string()),
            Some(r"C:\certs\client.pem".to_string()),
            Some(r"C:\certs\client-key.pem".to_string()),
        );
        let parsed = parse_mysql_dsn(&build_mysql_dsn(&config)).unwrap();

        assert_eq!(parsed.ssl_mode, MySqlSslMode::VerifyIdentity);
        assert_eq!(parsed.ssl_ca.as_deref(), Some(r"C:\certs\ca #1.pem"));
        assert_eq!(parsed.ssl_cert.as_deref(), Some(r"C:\certs\client.pem"));
        assert_eq!(parsed.ssl_key.as_deref(), Some(r"C:\certs\client-key.pem"));
    }

    #[test]
    fn ipv6_host_round_trip_serializes_without_url_brackets() {
        let config = MySqlConnectionConfig::new(
            "::1",
            3306,
            None,
            "user",
            "password",
            MySqlSslMode::Disabled,
        );
        let parsed = parse_mysql_dsn(&build_mysql_dsn(&config)).unwrap();

        assert_eq!(parsed.host, "::1");
        assert!(serialize_option_file(&parsed).contains("host = \"::1\"\n"));
    }

    #[test]
    fn option_file_quotes_syntax_characters_and_windows_paths() {
        let target = MySqlDsn {
            host: "localhost".to_string(),
            port: 3306,
            database: Some("app".to_string()),
            username: "user".to_string(),
            password: "p a#ss;=\"\\word".to_string(),
            ssl_mode: MySqlSslMode::Preferred,
            ssl_ca: None,
            ssl_cert: None,
            ssl_key: None,
        };
        let contents = serialize_option_file(&target);

        assert!(contents.contains("password = \"p a#ss;=\\\"\\\\word\""));
        let mut certificate = String::new();
        push_option(&mut certificate, "ssl-ca", r"C:\certs\server.pem");
        assert_eq!(certificate, "ssl-ca = \"C:\\\\certs\\\\server.pem\"\n");
        assert_eq!(
            quote_option_value(r"C:\certs\server.pem"),
            r#""C:\\certs\\server.pem""#
        );
    }

    #[test]
    fn server_database_listing_option_file_omits_selected_database() {
        let mut target = parse_mysql_dsn("mysql://user:password@localhost:3306/app").unwrap();
        target.database = None;

        let contents = serialize_option_file(&target);

        assert!(!contents.contains("database ="));
    }

    #[test]
    fn option_file_serializes_tls_paths_without_option_syntax_confusion() {
        let ca_path = r#" C:\certs\ca #1;= "quoted".pem "#;
        let target = MySqlDsn {
            host: "localhost".to_string(),
            port: 3306,
            database: None,
            username: "user".to_string(),
            password: "password".to_string(),
            ssl_mode: MySqlSslMode::VerifyCa,
            ssl_ca: Some(ca_path.to_string()),
            ssl_cert: Some(r"C:\certs\client.pem".to_string()),
            ssl_key: Some(r"C:\certs\client-key.pem".to_string()),
        };

        let contents = serialize_option_file(&target);

        assert!(contents.contains("ssl-mode = \"VERIFY_CA\"\n"));
        assert!(contents.contains(&format!("ssl-ca = {}\n", quote_option_value(ca_path))));
        assert!(contents.contains("ssl-cert = \"C:\\\\certs\\\\client.pem\"\n"));
        assert!(contents.contains("ssl-key = \"C:\\\\certs\\\\client-key.pem\"\n"));
    }

    #[test]
    fn rejects_control_characters_in_tls_paths_before_option_file_creation() {
        for field in ["CA", "client certificate", "client key"] {
            let mut target = MySqlDsn {
                host: "localhost".to_string(),
                port: 3306,
                database: None,
                username: "user".to_string(),
                password: "password".to_string(),
                ssl_mode: MySqlSslMode::Disabled,
                ssl_ca: None,
                ssl_cert: None,
                ssl_key: None,
            };
            match field {
                "CA" => target.ssl_ca = Some("ca\n.pem".to_string()),
                "client certificate" => target.ssl_cert = Some("client\r.pem".to_string()),
                "client key" => target.ssl_key = Some("client\0-key.pem".to_string()),
                _ => unreachable!(),
            }

            assert!(
                matches!(
                    MySqlOptionFile::create(&target),
                    Err(DbOperationError::ConnectionFailed(details))
                        if details == "MySQL connection settings contain a control character"
                ),
                "{field}"
            );
        }
    }

    #[test]
    fn rejects_encrypted_client_keys_before_process_start() {
        let directory = tempfile::tempdir().unwrap();
        let ca = directory.path().join("ca.pem");
        let cert = directory.path().join("client.pem");
        let key = directory.path().join("client-key.pem");
        fs::write(&ca, "-----BEGIN CERTIFICATE-----\nca\n").unwrap();
        fs::write(&cert, "-----BEGIN CERTIFICATE-----\ncert\n").unwrap();
        fs::write(&key, "-----BEGIN ENCRYPTED PRIVATE KEY-----\nsecret\n").unwrap();
        let target = MySqlDsn {
            host: "localhost".to_string(),
            port: 3306,
            database: None,
            username: "user".to_string(),
            password: "password".to_string(),
            ssl_mode: MySqlSslMode::VerifyCa,
            ssl_ca: Some(ca.display().to_string()),
            ssl_cert: Some(cert.display().to_string()),
            ssl_key: Some(key.display().to_string()),
        };

        let result = validate_mysql_tls_files(&target);

        assert!(matches!(
            result,
            Err(DbOperationError::ConnectionFailed(details))
                if details == "Encrypted MySQL client keys are not supported"
        ));
    }

    #[test]
    fn rejects_traditional_encrypted_client_keys_before_process_start() {
        let directory = tempfile::tempdir().unwrap();
        let ca = directory.path().join("ca.pem");
        let cert = directory.path().join("client.pem");
        let key = directory.path().join("client-key.pem");
        fs::write(&ca, "-----BEGIN CERTIFICATE-----\nca\n").unwrap();
        fs::write(&cert, "-----BEGIN CERTIFICATE-----\ncert\n").unwrap();
        fs::write(&key, "Proc-Type: 4,ENCRYPTED\nDEK-Info: AES-256-CBC,x\n").unwrap();
        let target = MySqlDsn {
            host: "localhost".to_string(),
            port: 3306,
            database: None,
            username: "user".to_string(),
            password: "password".to_string(),
            ssl_mode: MySqlSslMode::VerifyCa,
            ssl_ca: Some(ca.display().to_string()),
            ssl_cert: Some(cert.display().to_string()),
            ssl_key: Some(key.display().to_string()),
        };

        assert!(validate_mysql_tls_files(&target).is_err());
    }

    #[test]
    fn option_file_is_owner_only_and_removed_on_drop() {
        let target = MySqlDsn {
            host: "localhost".to_string(),
            port: 3306,
            database: None,
            username: "user".to_string(),
            password: "secret".to_string(),
            ssl_mode: MySqlSslMode::Disabled,
            ssl_ca: None,
            ssl_cert: None,
            ssl_key: None,
        };
        let option_file = MySqlOptionFile::create(&target).unwrap();
        assert!(option_file.path.is_absolute());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&option_file.path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let path = option_file.path.clone();
        drop(option_file);
        assert!(!path.exists());
    }

    #[test]
    fn option_file_names_are_unique_uuid_v4_paths_under_concurrency() {
        use std::collections::HashSet;
        use std::sync::{Arc, Barrier};

        fn target() -> MySqlDsn {
            MySqlDsn {
                host: "localhost".to_string(),
                port: 3306,
                database: None,
                username: "user".to_string(),
                password: "secret".to_string(),
                ssl_mode: MySqlSslMode::Disabled,
                ssl_ca: None,
                ssl_cert: None,
                ssl_key: None,
            }
        }

        let barrier = Arc::new(Barrier::new(16));
        let mut handles = Vec::with_capacity(16);
        for _ in 0..16 {
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let target = target();
                MySqlOptionFile::create(&target).unwrap()
            }));
        }
        let files = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        let paths = files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let unique_paths = paths.iter().collect::<HashSet<_>>();

        assert_eq!(unique_paths.len(), paths.len());
        for path in &paths {
            let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap();
            let uuid = stem.strip_prefix("sabiql-mysql-").unwrap();
            assert_eq!(uuid::Uuid::parse_str(uuid).unwrap().get_version_num(), 4);
        }

        drop(files);
        assert!(paths.iter().all(|path| !path.exists()));
    }

    #[test]
    fn option_file_is_removed_when_mysql_process_start_fails() {
        let (result, path) = {
            let target = MySqlDsn {
                host: "localhost".to_string(),
                port: 3306,
                database: None,
                username: "user".to_string(),
                password: "secret".to_string(),
                ssl_mode: MySqlSslMode::Disabled,
                ssl_ca: None,
                ssl_cert: None,
                ssl_key: None,
            };
            let option_file = MySqlOptionFile::create(&target).unwrap();
            let path = option_file.path.clone();
            let result = MysqlProcess::spawn_with_program(
                OsStr::new("__sabiql_missing_mysql_binary__"),
                &path,
            );
            (result, path)
        };

        assert!(result.is_err());
        assert!(!path.exists());
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

#[cfg(test)]
mod query_tests {
    use sabiql_app::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};

    use super::*;

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
    fn scanner_ignores_semicolons_in_quotes_and_comments() {
        let query = r#"
            SELECT 'a; b', "quoted; identifier", `back;tick`
            /* block ; comment */
            FROM `table` -- line ; comment
            WHERE value = 'backslash\\;'
        "#;

        assert!(validate_mysql_adhoc_query(query).is_ok());
    }

    #[test]
    fn scanner_rejects_multiple_statements_before_starting_a_process() {
        for query in ["SELECT 1; SELECT 2", "SELECT 1;\nSHOW TABLES"] {
            assert!(matches!(
                validate_mysql_adhoc_query(query),
                Err(DbOperationError::UnsupportedOperation(details))
                    if details.contains("multiple SQL statements")
            ));
        }
    }

    #[test]
    fn scanner_accepts_read_statements_and_select_ctes_only() {
        for query in [
            "SELECT 1",
            "TABLE users",
            "SHOW TABLES",
            "DESCRIBE users",
            "WITH rows AS (SELECT 1) SELECT * FROM rows",
            "WITH RECURSIVE rows AS (SELECT 1) SELECT * FROM rows",
        ] {
            assert!(validate_mysql_adhoc_query(query).is_ok(), "{query}");
        }
        for query in [
            "INSERT INTO users VALUES (1)",
            "WITH rows AS (SELECT 1) UPDATE users SET id = 2",
            "WITH rows AS (SELECT 1) SHOW TABLES",
        ] {
            assert!(validate_mysql_adhoc_query(query).is_err(), "{query}");
        }
    }

    #[test]
    fn scanner_rejects_top_level_into_clauses_before_starting_a_process() {
        for query in [
            "SELECT id INTO OUTFILE '/tmp/result' FROM users",
            "SELECT id INTO DUMPFILE '/tmp/result' FROM users",
            "SELECT id INTO @value FROM users",
            "TABLE users INTO OUTFILE '/tmp/result'",
            "WITH rows AS (SELECT 1) SELECT * INTO OUTFILE '/tmp/result' FROM rows",
        ] {
            assert!(matches!(
                validate_mysql_adhoc_query(query),
                Err(DbOperationError::UnsupportedOperation(details))
                    if details.contains("SELECT INTO clauses")
            ));
        }
        assert!(
            validate_mysql_adhoc_query("WITH rows AS (SELECT 'INTO OUTFILE') SELECT * FROM rows")
                .is_ok()
        );
    }

    #[test]
    fn scanner_rejects_client_commands_and_version_comments() {
        for query in [
            "DELIMITER //\nSELECT 1//",
            "  charset utf8mb4\nSELECT 1",
            "source ./script.sql",
            "system echo unsafe",
            "\\C /tmp/other.sock\nSELECT 1",
            "SELECT 1 /*!40101 + 1 */",
        ] {
            assert!(matches!(
                validate_mysql_adhoc_query(query),
                Err(DbOperationError::UnsupportedOperation(_))
            ));
        }
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
            "--batch",
            "--column-names",
            "--column-type-info",
            "--binary-as-hex",
            "--binary-mode",
            "--unbuffered",
            "--skip-reconnect",
            "--default-character-set=utf8mb4",
        ] {
            assert!(args.contains(&expected.to_string()), "{expected}");
        }
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
            "invalid" => "printf '%s\\t%s\\n' wrong wrong; printf '%s\\t%s\\n' x x".to_string(),
            "unsupported" => "emit_probe \"$marker\" ANSI_QUOTES".to_string(),
            "timeout" => "while :; do :; done".to_string(),
            _ => "emit_probe \"$marker\" STRICT_TRANS_TABLES".to_string(),
        };
        let user_response = if mode == "failure" {
            "printf '%s\\t%s\\n' partial row\n    printf '%s\\n' 'ERROR 1064 (42000): syntax error' >&2\n    exit 1"
        } else if mode == "no_result_failure" {
            "printf '%s\\n' 'ERROR 1054 (42S22): Unknown column missing_column' >&2\n    exit 1"
        } else {
            "printf '%s\\n' value; printf '%s\\n' ok"
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
emit_probe() {{
  printf '%s\t%s\n' __sabiql_probe __sabiql_sql_mode
  printf '%s\t%s\n' "$1" "$2"
}}
emit_marker() {{
  printf '%s\t%s\n' __sabiql_marker affected_rows
  printf '%s\t%s\n' "$1" 0
}}
emit_session() {{
  printf '%s\n' __sabiql_session_marker
  printf '%s\n' "$1"
}}
phase=probe
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  [ "$line" = ";" ] && continue
  if [ "$phase" = probe ]; then
    marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)'.*/\\1/")
    {probe_response}
    phase=probe_marker
  elif [ "$phase" = probe_marker ]; then
    case "$line" in
      *__sabiql_marker*)
        marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)'.*/\\1/")
        emit_marker "$marker"
        phase=ready
        ;;
    esac
  elif [ "$phase" = user_marker ]; then
    case "$line" in
      *__sabiql_marker*)
        marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)'.*/\\1/")
        emit_marker "$marker"
        exit 0
        ;;
    esac
  else
    case "$line" in
      "SET SESSION TRANSACTION READ ONLY")
        {session_failure}
        ;;
      *__sabiql_session_marker*)
        marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
        emit_session "$marker"
        phase=session_marker
        ;;
      session_marker)
        case "$line" in
          *__sabiql_marker*)
            marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)'.*/\\1/")
            emit_marker "$marker"
            phase=ready
            ;;
        esac
        ;;
      *__sabiql_marker*)
        marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)'.*/\\1/")
        emit_marker "$marker"
        ;;
      *)
        {user_response}
        phase=user_marker
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
            || "printf '%s\\t%s\\n' affected ok".to_string(),
            |error| format!("printf '%s\\n' '{error}' >&2"),
        );
        let marker_response = if marker_failure {
            "marker_count=$((marker_count + 1))
      if [ \"$marker_count\" -eq 1 ]; then
        marker=$(printf '%s\\n' \"$line\" | sed \"s/.*SELECT '\\\\([^']*\\\\)' AS __sabiql_marker.*/\\\\1/\")
        printf '%s\\t%s\\n' __sabiql_marker affected_rows
        printf '%s\\t%s\\n' \"$marker\" 0
      else
        printf '%s\\t%s\\n' wrong x
        exit 0
      fi"
        } else {
            "marker=$(printf '%s\\n' \"$line\" | sed \"s/.*SELECT '\\\\([^']*\\\\)' AS __sabiql_marker.*/\\\\1/\")
      rows=0
      case \"$line\" in *ROW_COUNT\\(\\)* ) rows=3 ;; esac
      printf '%s\\t%s\\n' __sabiql_marker affected_rows
      printf '%s\\t%s\\n' \"$marker\" \"$rows\"
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
marker_count=0
emit_probe() {{
  printf '%s\t%s\n' __sabiql_probe __sabiql_sql_mode
  printf '%s\t%s\n' "$1" STRICT_TRANS_TABLES
}}
emit_session() {{
  printf '%s\n' __sabiql_session_marker
  printf '%s\n' "$1"
}}
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    *__sabiql_probe*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_probe.*/\\1/")
      emit_probe "$marker"
      ;;
    "SET SESSION TRANSACTION READ ONLY")
      ;;
    *__sabiql_session_marker*)
      marker=$(printf '%s\n' "$line" | sed "s/.*SELECT '\\([^']*\\)' AS __sabiql_session_marker.*/\\1/")
      emit_session "$marker"
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
      printf '%s\n' value
      printf '%s\n' "$value"
      ;;
    *UPDATE*)
      {update_response}
      ;;
    *)
      printf '%s\n' value
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

    #[tokio::test]
    async fn exports_mysql_batch_rows_through_the_shared_csv_writer() {
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
