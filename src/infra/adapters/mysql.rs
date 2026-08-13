use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};

use async_trait::async_trait;
use quick_xml::Reader;
use quick_xml::events::Event;
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

use crate::app::ports::outbound::{
    AccessMode, ConnectionProbe, DatabaseCli, DbOperationError, DdlGenerator, DsnBuilder,
    MYSQL_CLI_VERSION_REQUIRED_MARKER, MYSQL_SERVER_VERSION_REQUIRED_MARKER,
    MYSQL_SQL_MODE_UNSUPPORTED_MARKER, QueryExecutor, SqlDialect,
};
use crate::domain::connection::{
    ConnectionProfile, DatabaseType, MySqlConnectionConfig, MySqlSslMode,
};
use crate::domain::{QueryResult, QuerySource, QueryValue, Table, WriteExecutionResult};

mod metadata;

pub struct MySqlAdapter;

const MYSQL_PROBE_TIMEOUT: Duration = Duration::from_secs(11);
const MYSQL_QUERY_TIMEOUT: Duration = Duration::from_secs(31);
const MYSQL_PROBE_QUERY: &str = "SELECT JSON_OBJECT('database', DATABASE(), 'user', CURRENT_USER(), 'version', VERSION(), 'sql_mode', @@SESSION.sql_mode)";
static OPTION_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
            limit,
            offset,
        );
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, &query).await;
        drop(option_file);
        let result_set = result?;
        let values = metadata::convert_preview_values(&result_set, &preview.visible_columns)?;
        let elapsed = start.elapsed().as_millis() as u64;

        Ok(QueryResult::success_with_values(
            query,
            preview
                .visible_columns
                .iter()
                .map(|column| column.name.clone())
                .collect(),
            values,
            elapsed,
            QuerySource::Preview,
        ))
    }

    async fn execute_adhoc(
        &self,
        dsn: &str,
        query: &str,
        _access_mode: AccessMode,
    ) -> Result<QueryResult, DbOperationError> {
        validate_mysql_adhoc_query(query)?;
        let target = parse_mysql_dsn(dsn)?;
        validate_mysql_values(&target)?;
        validate_mysql_tls_files(&target)?;

        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures mysql execution time at the I/O boundary"
        )]
        let start = Instant::now();
        let option_file = MySqlOptionFile::create(&target)?;
        let result = run_mysql_adhoc(&option_file.path, query).await;
        drop(option_file);
        let result_set = result?;
        let elapsed = start.elapsed().as_millis() as u64;

        Ok(QueryResult::success_with_values(
            query.to_string(),
            result_set.columns,
            result_set.values,
            elapsed,
            QuerySource::Adhoc,
        ))
    }

    async fn execute_write(
        &self,
        _dsn: &str,
        _query: &str,
        _access_mode: AccessMode,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL query execution is not implemented".to_string(),
        ))
    }

    async fn count_query_rows(&self, _dsn: &str, _query: &str) -> Result<usize, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL query execution is not implemented".to_string(),
        ))
    }

    async fn export_to_csv(
        &self,
        _dsn: &str,
        _query: &str,
        _file_name: &str,
    ) -> Result<std::path::PathBuf, DbOperationError> {
        Err(DbOperationError::ConnectionFailed(
            "MySQL query execution is not implemented".to_string(),
        ))
    }
}

impl DdlGenerator for MySqlAdapter {
    fn generate_ddl(&self, _database_type: DatabaseType, _table: &Table) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }
}

impl SqlDialect for MySqlAdapter {
    fn build_explain_sql(&self, _database_type: DatabaseType, _query: &str) -> Option<String> {
        None
    }

    fn build_explain_analyze_sql(
        &self,
        _database_type: DatabaseType,
        _query: &str,
    ) -> Option<String> {
        None
    }

    fn build_update_sql(
        &self,
        _database_type: DatabaseType,
        _schema: &str,
        _table: &str,
        _column: &str,
        _new_value: &QueryValue,
        _pk_pairs: &[(String, QueryValue)],
    ) -> String {
        unimplemented!("MySQL adapter not yet implemented")
    }

    fn build_bulk_delete_sql(
        &self,
        _database_type: DatabaseType,
        _schema: &str,
        _table: &str,
        _pk_pairs_per_row: &[Vec<(String, QueryValue)>],
    ) -> String {
        unimplemented!("MySQL adapter not yet implemented")
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
        let result = run_mysql_adhoc(&option_file.path, "SHOW DATABASES").await;
        drop(option_file);
        result.map(|result| {
            result
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
        target.host.as_str(),
        target.username.as_str(),
        target.password.as_str(),
    ];
    if target
        .database
        .as_deref()
        .into_iter()
        .chain(values)
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
        validate_mysql_tls_files(target)?;
        let sequence = OPTION_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "sabiql-mysql-{}-{sequence}.cnf",
            std::process::id()
        ));
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
) -> Result<MysqlResultSet, DbOperationError> {
    run_mysql_adhoc_with_program(OsStr::new("mysql"), option_file, query, MYSQL_QUERY_TIMEOUT).await
}

async fn run_mysql_adhoc_with_program(
    program: &OsStr,
    option_file: &std::path::Path,
    query: &str,
    execution_timeout: Duration,
) -> Result<MysqlResultSet, DbOperationError> {
    let mut process = MysqlProcess::spawn_with_program(program, option_file)?;
    let result = timeout(
        execution_timeout,
        run_mysql_adhoc_process(&mut process, query),
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
    query: &str,
) -> Result<MysqlResultSet, DbOperationError> {
    let marker = Uuid::new_v4().simple().to_string();
    let probe_query =
        format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
    write_mysql_statement(process, &probe_query).await?;
    let probe_xml = read_one_mysql_resultset(process).await?;
    let probe = parse_mysql_xml(&probe_xml)?;
    validate_mode_probe(&probe, &marker)?;

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
        write_mysql_input(process, b"\\q\n").await?;
        let tail = read_pty_all(&mut process.pty)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        trace_mysql_frame("discard tail", tail.len());
        trace_mysql_error(&tail);
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

async fn write_mysql_statement(
    process: &mut MysqlProcess,
    query: &str,
) -> Result<(), DbOperationError> {
    let query = query.trim_end();
    write_mysql_input(process, query.as_bytes()).await?;
    if query.ends_with(';') {
        write_mysql_input(process, b"\n").await
    } else {
        write_mysql_input(process, b";\n").await
    }
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
    read_one_mysql_resultset_from_pipes(&mut process.stdout, &mut process.stderr).await
}

#[cfg(unix)]
async fn read_one_pty_resultset(pty: &mut MysqlPty) -> Result<Vec<u8>, DbOperationError> {
    let mut chunk = [0; 4096];
    loop {
        if let Some(frame) = take_mysql_resultset_frame(&mut pty.pending) {
            trace_mysql_frame("receive resultset", frame.len());
            return Ok(frame);
        }
        if has_mysql_cli_error(&pty.pending) {
            trace_mysql_error(&pty.pending);
            return Err(classify_mysql_query_failure(&pty.pending));
        }
        let count = match pty.output.read(&mut chunk).await {
            Ok(count) => count,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => 0,
            Err(error) => return Err(DbOperationError::ConnectionLost(error.to_string())),
        };
        if count == 0 {
            return Err(DbOperationError::EmptyResponse(
                "mysql query returned no resultset".to_string(),
            ));
        }
        pty.pending.extend_from_slice(&chunk[..count]);
    }
}

#[cfg(unix)]
async fn read_pty_all(pty: &mut MysqlPty) -> io::Result<Vec<u8>> {
    let mut output = std::mem::take(&mut pty.pending);
    let mut chunk = [0; 4096];
    loop {
        match pty.output.read(&mut chunk).await {
            Ok(0) => return Ok(output),
            Ok(count) => output.extend_from_slice(&chunk[..count]),
            Err(error) if error.raw_os_error() == Some(libc::EIO) => return Ok(output),
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
fn take_mysql_resultset_frame(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let start = [&b"<?xml"[..], &b"<resultset"[..]]
        .iter()
        .filter_map(|prefix| find_bytes(buffer, prefix))
        .min()?;
    let end = buffer[start..]
        .windows(b"</resultset>".len())
        .position(|window| window == b"</resultset>")?
        + start
        + b"</resultset>".len();
    let frame = buffer[start..end].to_vec();
    buffer.drain(..end);
    Some(frame)
}

#[cfg(unix)]
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn trace_mysql_frame(kind: &str, bytes: usize) {
    if std::env::var_os("SABIQL_MYSQL_TRANSCRIPT").is_some() {
        write_mysql_transcript_line(&format!("sabiql mysql frame: {kind}, bytes={bytes}"));
    }
}

fn trace_mysql_error(output: &[u8]) {
    if std::env::var_os("SABIQL_MYSQL_TRANSCRIPT").is_some() && has_mysql_cli_error(output) {
        write_mysql_transcript_line("sabiql mysql frame: ERROR line observed");
    }
}

fn write_mysql_transcript_line(line: &str) {
    let mut stderr = io::stderr();
    let _ = stderr.write_all(line.as_bytes());
    let _ = stderr.write_all(b"\n");
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

#[cfg(not(unix))]
async fn read_one_mysql_resultset_from_pipes<R, E>(
    reader: &mut R,
    stderr: &mut E,
) -> Result<Vec<u8>, DbOperationError>
where
    R: AsyncRead + Unpin,
    E: AsyncRead + Unpin,
{
    const RESULTSET_END: &[u8] = b"</resultset>";
    let mut output = Vec::new();
    let mut chunk = [0; 4096];
    let mut stderr_chunk = [0; 4096];
    let mut stderr_closed = false;
    loop {
        if stderr_closed {
            let count = reader
                .read(&mut chunk)
                .await
                .map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
            if count == 0 {
                return Err(DbOperationError::EmptyResponse(
                    "mysql mode probe returned no resultset".to_string(),
                ));
            }
            output.extend_from_slice(&chunk[..count]);
        } else {
            tokio::select! {
                result = reader.read(&mut chunk) => {
                    let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                    if count == 0 {
                        return Err(DbOperationError::EmptyResponse(
                            "mysql mode probe returned no resultset".to_string(),
                        ));
                    }
                    output.extend_from_slice(&chunk[..count]);
                }
                result = stderr.read(&mut stderr_chunk) => {
                    let count = result.map_err(|error| DbOperationError::ConnectionLost(error.to_string()))?;
                    if count == 0 {
                        stderr_closed = true;
                    } else {
                        let details = String::from_utf8_lossy(&stderr_chunk[..count]);
                        let lower = details.to_ascii_lowercase();
                        if lower.contains("error")
                            || lower.contains("access denied")
                            || lower.contains("can't connect")
                            || lower.contains("lost connection")
                        {
                            return Err(classify_mysql_query_failure(&stderr_chunk[..count]));
                        }
                    }
                }
            }
        }
        if output
            .windows(RESULTSET_END.len())
            .any(|window| window == RESULTSET_END)
        {
            return Ok(output);
        }
    }
}

fn mysql_query_args(option_file: &std::path::Path) -> Vec<String> {
    let args = vec![
        format!("--defaults-file={}", option_file.display()),
        "--no-login-paths".to_string(),
        "--protocol=TCP".to_string(),
        "--connect-timeout=10".to_string(),
        "--xml".to_string(),
        "--binary-as-hex".to_string(),
        "--binary-mode".to_string(),
        "--unbuffered".to_string(),
        "--skip-reconnect".to_string(),
        "--default-character-set=utf8mb4".to_string(),
        "--silent".to_string(),
        "--prompt=".to_string(),
    ];
    #[cfg(not(unix))]
    return args
        .into_iter()
        .chain(std::iter::once("--batch".to_string()))
        .collect();
    #[cfg(unix)]
    args
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

#[derive(Debug, PartialEq, Eq)]
enum MysqlToken {
    Word(String),
    OpenParen,
    CloseParen,
    Comma,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MysqlReadStatement {
    Select,
    Table,
    Show,
    Describe,
}

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

fn unsupported_client_command(command: &str) -> DbOperationError {
    DbOperationError::UnsupportedOperation(format!("unsupported MySQL {command}"))
}

fn parse_mysql_xml(xml: &[u8]) -> Result<MysqlResultSet, DbOperationError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut resultset_count = 0;
    let mut in_resultset = false;
    let mut current_row: Option<Vec<(String, QueryValue)>> = None;
    let mut current_field: Option<MysqlField> = None;
    let mut rows = Vec::new();
    let mut columns = Vec::new();

    loop {
        let event = reader.read_event_into(&mut buffer).map_err(|error| {
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
                let field = parse_mysql_field(&element)?;
                row.push(field.finish());
            }
            Event::Text(text) => {
                let text = text.unescape().map_err(|error| {
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
                    row.push(field.finish());
                }
                b"row" => {
                    let row = current_row.take().ok_or_else(|| {
                        DbOperationError::QueryFailed("unexpected MySQL XML row end".to_string())
                    })?;
                    if columns.is_empty() {
                        columns = row.iter().map(|(name, _)| name.clone()).collect();
                    } else if row.len() != columns.len()
                        || row
                            .iter()
                            .zip(&columns)
                            .any(|((name, _), column)| name != column)
                    {
                        return Err(DbOperationError::QueryFailed(
                            "MySQL XML rows have inconsistent fields".to_string(),
                        ));
                    }
                    rows.push(row.into_iter().map(|(_, value)| value).collect());
                }
                b"resultset" => {
                    if !in_resultset || current_row.is_some() || current_field.is_some() {
                        return Err(DbOperationError::QueryFailed(
                            "malformed MySQL XML resultset".to_string(),
                        ));
                    }
                    in_resultset = false;
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
    Ok(MysqlResultSet {
        columns,
        values: rows,
    })
}

struct MysqlField {
    name: String,
    value: String,
    is_null: bool,
}

impl MysqlField {
    fn finish(self) -> (String, QueryValue) {
        let value = if self.is_null {
            QueryValue::Null
        } else {
            QueryValue::Text(self.value)
        };
        (self.name, value)
    }
}

fn parse_mysql_field(
    element: &quick_xml::events::BytesStart<'_>,
) -> Result<MysqlField, DbOperationError> {
    let mut name = None;
    let mut is_null = false;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| {
            DbOperationError::QueryFailed(format!("invalid MySQL XML field: {error}"))
        })?;
        let value = attribute.unescape_value().map_err(|error| {
            DbOperationError::QueryFailed(format!("invalid MySQL XML field: {error}"))
        })?;
        match attribute.key.as_ref() {
            b"name" => name = Some(value.into_owned()),
            b"xsi:nil" | b"nil" => is_null = matches!(value.as_ref(), "true" | "1"),
            _ => {}
        }
    }
    let name = name
        .ok_or_else(|| DbOperationError::QueryFailed("MySQL XML field has no name".to_string()))?;
    Ok(MysqlField {
        name,
        value: String::new(),
        is_null,
    })
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
        let target = MySqlDsn {
            host: "localhost".to_string(),
            port: 3306,
            database: None,
            username: "user".to_string(),
            password: "password".to_string(),
            ssl_mode: MySqlSslMode::VerifyCa,
            ssl_ca: Some(r"C:\certs\ca #1.pem".to_string()),
            ssl_cert: Some(r"C:\certs\client.pem".to_string()),
            ssl_key: Some(r"C:\certs\client-key.pem".to_string()),
        };

        let contents = serialize_option_file(&target);

        assert!(contents.contains("ssl-mode = \"VERIFY_CA\"\n"));
        assert!(contents.contains("ssl-ca = \"C:\\\\certs\\\\ca #1.pem\"\n"));
        assert!(contents.contains("ssl-cert = \"C:\\\\certs\\\\client.pem\"\n"));
        assert!(contents.contains("ssl-key = \"C:\\\\certs\\\\client-key.pem\"\n"));
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
            "--xml",
            "--binary-as-hex",
            "--binary-mode",
            "--unbuffered",
            "--skip-reconnect",
            "--default-character-set=utf8mb4",
        ] {
            assert!(args.contains(&expected.to_string()), "{expected}");
        }
        #[cfg(unix)]
        {
            assert!(args.contains(&"--silent".to_string()));
            assert!(args.contains(&"--prompt=".to_string()));
            assert!(!args.contains(&"--batch".to_string()));
        }
        #[cfg(not(unix))]
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
        } else {
            "printf '%s\\n' '<resultset><row><field name=\"value\">ok</field></row></resultset>'"
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
    {user_response}
    exit 0
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

    #[tokio::test]
    async fn sends_user_sql_only_after_a_valid_mode_probe() {
        let (_directory, program, log_file) = fake_mysql("success");
        let option_file = log_file.with_extension("cnf");
        fs::write(&option_file, "[client]\n").unwrap();
        let result = run_mysql_adhoc_with_program(
            OsStr::new(&program),
            &option_file,
            "SELECT 123",
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert_eq!(result.columns, vec!["value"]);
        assert_eq!(result.values[0][0].as_str(), Some("ok"));
        let log = fs::read_to_string(format!("{}.log", option_file.display())).unwrap();
        assert!(log.contains("__sabiql_probe"));
        assert!(log.contains("SELECT 123"));
    }

    #[test]
    fn frames_one_xml_resultset_and_preserves_following_output() {
        let mut buffer = b"    -> <?xml version=\"1.0\"?>\n<resultset></resultset>\r\n    -> <?xml version=\"1.0\"?>\n<resultset>"
            .to_vec();

        assert_eq!(
            take_mysql_resultset_frame(&mut buffer),
            Some(b"<?xml version=\"1.0\"?>\n<resultset></resultset>".to_vec())
        );
        assert_eq!(
            take_mysql_resultset_frame(&mut buffer),
            None,
            "an incomplete following frame must remain buffered"
        );
        assert!(buffer.starts_with(b"\r\n    -> <?xml"));
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
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
    }
}
