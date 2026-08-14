use std::fs;

use url::Url;

use crate::app::ports::outbound::{DbOperationError, DsnBuilder};
use crate::domain::connection::{ConnectionProfile, MySqlConnectionConfig, MySqlSslMode};

use super::adapter::MySqlAdapter;

#[derive(Debug)]
pub(super) struct MySqlDsn {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) database: Option<String>,
    pub(super) username: String,
    pub(super) password: String,
    pub(super) ssl_mode: MySqlSslMode,
    pub(super) ssl_ca: Option<String>,
    pub(super) ssl_cert: Option<String>,
    pub(super) ssl_key: Option<String>,
}

pub(super) fn build_mysql_dsn(config: &MySqlConnectionConfig) -> String {
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

impl DsnBuilder for MySqlAdapter {
    fn build_dsn(&self, profile: &ConnectionProfile) -> String {
        let config = profile
            .mysql_config()
            .expect("MySQL profile requires MySQL config");
        build_mysql_dsn(config)
    }
}

pub(super) fn parse_mysql_dsn(dsn: &str) -> Result<MySqlDsn, DbOperationError> {
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

pub(super) fn validate_mysql_values(target: &MySqlDsn) -> Result<(), DbOperationError> {
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

pub(super) fn validate_mysql_tls_files(target: &MySqlDsn) -> Result<(), DbOperationError> {
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

#[cfg(test)]
mod tests {
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
    fn ipv6_host_round_trip_normalizes_url_brackets() {
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
    }

    #[test]
    fn rejects_control_characters_in_tls_paths() {
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
                    validate_mysql_values(&target),
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
}
