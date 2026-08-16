use std::ffi::OsStr;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::outbound::{
    ConnectionFailureKind, DatabaseCli, DbOperationError, MYSQL_CONNECT_TIMEOUT_ERRNOS,
    UnsupportedOperationKind,
};

const MYSQL_PROBE_TIMEOUT: Duration = Duration::from_secs(11);
const MYSQL_PROBE_QUERY: &str = "SELECT JSON_OBJECT('database', DATABASE(), 'user', CURRENT_USER(), 'version', VERSION(), 'sql_mode', @@SESSION.sql_mode)";

#[derive(Debug, Deserialize)]
struct MySqlProbeResponse {
    database: Option<String>,
    user: String,
    version: String,
    sql_mode: String,
}

pub(in crate::adapters::mysql) async fn check_mysql_cli_version() -> Result<(), DbOperationError> {
    let output = run_mysql_command(["--version"], None).await?;
    let version_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() || !is_oracle_mysql_cli_84_version(&version_output) {
        return Err(DbOperationError::UnsupportedOperationWithKind {
            kind: UnsupportedOperationKind::ClientVersion,
            details: version_output.trim().to_string(),
        });
    }
    Ok(())
}

pub(in crate::adapters::mysql) async fn probe_mysql_server(
    option_file: &PathBuf,
) -> Result<(), DbOperationError> {
    let output = run_mysql_command(mysql_probe_args(option_file), Some(option_file)).await?;
    if !output.status.success() {
        return Err(classify_mysql_probe_failure(clean_stderr(&output.stderr)));
    }

    let response: MySqlProbeResponse = serde_json::from_slice(&output.stdout)?;
    let _ = (&response.database, &response.user);
    validate_server_version(&response.version)?;
    validate_sql_mode(&response.sql_mode)
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
        Err(DbOperationError::UnsupportedOperationWithKind {
            kind: UnsupportedOperationKind::ServerVersion,
            details: version.to_string(),
        })
    }
}

pub(super) fn validate_sql_mode(sql_mode: &str) -> Result<(), DbOperationError> {
    let unsupported = sql_mode.split(',').map(str::trim).any(|mode| {
        mode.eq_ignore_ascii_case("NO_BACKSLASH_ESCAPES")
            || mode.eq_ignore_ascii_case("ANSI_QUOTES")
    });
    if unsupported {
        Err(DbOperationError::UnsupportedOperationWithKind {
            kind: UnsupportedOperationKind::SessionMode,
            details: sql_mode.to_string(),
        })
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

pub(super) async fn run_mysql_command<I, S>(
    args: I,
    option_file: Option<&PathBuf>,
) -> Result<std::process::Output, DbOperationError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_mysql_command_with_timeout(
        args,
        option_file,
        MYSQL_PROBE_TIMEOUT,
        "mysql probe exceeded the connection timeout",
    )
    .await
}

pub(super) async fn run_mysql_command_with_timeout<I, S>(
    args: I,
    option_file: Option<&PathBuf>,
    command_timeout: Duration,
    timeout_message: &str,
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

    match timeout(command_timeout, command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) if error.kind() == io::ErrorKind::NotFound => {
            Err(DbOperationError::CommandNotFound {
                command: DatabaseCli::MySql,
                details: error.to_string(),
            })
        }
        Ok(Err(error)) => Err(DbOperationError::ConnectionFailed(error.to_string())),
        Err(_) => Err(DbOperationError::Timeout(timeout_message.to_string())),
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
    } else if let Some(kind) = mysql_tls_failure_kind(&stderr.to_ascii_lowercase()) {
        DbOperationError::ConnectionFailedWithKind {
            kind,
            details: stderr,
        }
    } else if stderr
        .to_ascii_lowercase()
        .contains("can't connect to mysql server")
    {
        DbOperationError::ConnectionFailed(stderr)
    } else {
        super::error::classify_mysql_query_failure(stderr.as_bytes())
    }
}

pub(super) fn mysql_tls_failure_kind(lowercase_details: &str) -> Option<ConnectionFailureKind> {
    if lowercase_details.contains("certificate required")
        || lowercase_details.contains("client certificate")
        || lowercase_details.contains("peer did not return a certificate")
        || lowercase_details.contains("bad certificate")
        || lowercase_details.contains("tlsv1 alert certificate required")
    {
        return Some(ConnectionFailureKind::TlsClientCertificateRejected);
    }

    if lowercase_details.contains("hostname mismatch")
        || lowercase_details.contains("host name mismatch")
        || lowercase_details.contains("hostname does not match")
        || lowercase_details.contains("host name does not match")
        || lowercase_details.contains("hostname verification failed")
        || lowercase_details.contains("host name verification failed")
        || lowercase_details.contains("certificate name mismatch")
        || lowercase_details.contains("certificate does not match")
        || lowercase_details.contains("does not match certificate")
        || lowercase_details.contains("not valid for the requested host")
        || lowercase_details.contains("not valid for hostname")
        || lowercase_details.contains("subject alternative name")
        || (lowercase_details.contains("verify identity")
            && lowercase_details.contains("certificate"))
    {
        return Some(ConnectionFailureKind::TlsHostnameVerification);
    }

    if lowercase_details.contains("unable to get local issuer")
        || lowercase_details.contains("self-signed certificate")
        || lowercase_details.contains("unknown ca")
        || lowercase_details.contains("certificate signature failure")
    {
        return Some(ConnectionFailureKind::TlsCaVerification);
    }

    if lowercase_details.contains("error:0a000086:ssl routines::certificate verify failed") {
        return Some(ConnectionFailureKind::TlsCertificateVerification);
    }

    if lowercase_details.contains("error 2026")
        || lowercase_details.contains("tls/ssl error")
        || lowercase_details.contains("ssl handshake")
        || lowercase_details.contains("tls handshake")
        || lowercase_details.contains("handshake failure")
        || lowercase_details.contains("ssl connection error")
        || lowercase_details.contains("tlsv1 alert")
    {
        return Some(ConnectionFailureKind::TlsHandshake);
    }

    None
}

pub(super) fn is_mysql_connect_timeout_message(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("can't connect to mysql server")
        && MYSQL_CONNECT_TIMEOUT_ERRNOS
            .iter()
            .any(|errno| lower.contains(errno))
}

#[cfg(test)]
mod probe_tests {
    use super::*;

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
        assert!(matches!(
            validate_server_version("8.0.36"),
            Err(DbOperationError::UnsupportedOperationWithKind {
                kind: UnsupportedOperationKind::ServerVersion,
                ..
            })
        ));
        assert!(validate_sql_mode("STRICT_TRANS_TABLES").is_ok());
        assert!(matches!(
            validate_sql_mode("STRICT_TRANS_TABLES,ANSI_QUOTES"),
            Err(DbOperationError::UnsupportedOperationWithKind {
                kind: UnsupportedOperationKind::SessionMode,
                ..
            })
        ));
    }

    #[test]
    fn rejects_no_backslash_escapes_sql_mode() {
        assert!(matches!(
            validate_sql_mode("STRICT_TRANS_TABLES,NO_BACKSLASH_ESCAPES"),
            Err(DbOperationError::UnsupportedOperationWithKind {
                kind: UnsupportedOperationKind::SessionMode,
                ..
            })
        ));
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
        assert!(matches!(
            classify_mysql_probe_failure(
                "ERROR 2003 (HY000): Can't connect to MySQL server on 'host:3306' (60)".to_string()
            ),
            DbOperationError::Timeout(_)
        ));
    }

    #[test]
    fn classifies_mysql_tls_probe_failure_for_connection_error() {
        let error = classify_mysql_probe_failure(
            "ERROR 2026 (HY000): SSL connection error: error:0A000086:SSL routines::certificate verify failed"
                .to_string(),
        );

        assert!(matches!(
            error,
            DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::TlsCertificateVerification,
                ..
            }
        ));
    }

    #[test]
    fn classifies_mysql_server_probe_failures_with_query_error_vocabulary() {
        assert!(matches!(
            classify_mysql_probe_failure(
                "ERROR 1044 (42000): Access denied for user 'user' to database 'mysql'".to_string()
            ),
            DbOperationError::PermissionDenied(_)
        ));
        assert!(matches!(
            classify_mysql_probe_failure(
                "ERROR 1045 (28000): Access denied for user 'user'".to_string()
            ),
            DbOperationError::ConnectionFailed(_)
        ));
        assert!(matches!(
            classify_mysql_probe_failure(
                "ERROR 1049 (42000): Unknown database 'missing'".to_string()
            ),
            DbOperationError::ConnectionFailed(_)
        ));
    }

    #[test]
    fn classifies_mysql_tls_failures_before_the_app_boundary() {
        for (stderr, expected) in [
            (
                "ERROR 2026 (HY000): TLS/SSL error: hostname mismatch",
                ConnectionFailureKind::TlsHostnameVerification,
            ),
            (
                "ERROR 2026 (HY000): TLS/SSL error: unable to get local issuer certificate",
                ConnectionFailureKind::TlsCaVerification,
            ),
            (
                "ERROR 2026 (HY000): TLS/SSL error: peer did not return a certificate",
                ConnectionFailureKind::TlsClientCertificateRejected,
            ),
            (
                "ERROR 2026 (HY000): SSL connection error",
                ConnectionFailureKind::TlsHandshake,
            ),
        ] {
            assert_eq!(
                mysql_tls_failure_kind(&stderr.to_ascii_lowercase()),
                Some(expected)
            );
        }
    }
}
