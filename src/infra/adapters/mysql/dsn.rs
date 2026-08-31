use std::{fmt, sync::Arc};

use url::Url;

use crate::app::ports::outbound::{ConnectionFailureKind, DbOperationError, DsnBuilder};
use crate::domain::connection::{
    ConnectionProfile, MySqlConnectionConfig, MySqlSslMode, MySqlTransport,
};

use super::adapter::MySqlAdapter;

pub(super) struct MySqlDsn {
    pub(super) transport: MySqlTransport,
    pub(super) transport_path: Option<String>,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) database: Option<String>,
    pub(super) username: String,
    pub(super) password: String,
    pub(super) ssl_mode: MySqlSslMode,
    pub(super) ssl_ca: Option<String>,
    pub(super) ssl_cert: Option<String>,
    pub(super) ssl_key: Option<String>,
    pub(super) server_public_key_path: Option<String>,
    pub(super) enable_cleartext_plugin: bool,
}

impl fmt::Debug for MySqlDsn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MySqlDsn")
            .field("transport", &self.transport)
            .field("transport_path", &self.transport_path)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("username", &self.username)
            .field("password", &"****")
            .field("ssl_mode", &self.ssl_mode)
            .field("ssl_ca", &self.ssl_ca)
            .field("ssl_cert", &self.ssl_cert)
            .field("ssl_key", &self.ssl_key)
            .field("server_public_key_path", &self.server_public_key_path)
            .field("enable_cleartext_plugin", &self.enable_cleartext_plugin)
            .finish()
    }
}

fn build_mysql_dsn(config: &MySqlConnectionConfig) -> String {
    let mut url = Url::parse("mysql://localhost").expect("static MySQL URL is valid");
    url.set_username(&config.username)
        .expect("MySQL username is valid URL data");
    url.set_password(Some(&config.password))
        .expect("MySQL password is valid URL data");
    if config.transport == MySqlTransport::Tcp {
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
    }
    if let Some(database) = config.database.as_deref() {
        url.path_segments_mut()
            .expect("MySQL URL supports path segments")
            .push(database);
    }
    url.query_pairs_mut()
        .append_pair("ssl-mode", &config.ssl_mode.to_string());
    if config.ssl_mode.uses_ca()
        && let Some(path) = config.ssl_ca.as_deref()
    {
        url.query_pairs_mut().append_pair("ssl-ca", path);
    }
    if let Some(path) = config.ssl_cert.as_deref() {
        url.query_pairs_mut().append_pair("ssl-cert", path);
    }
    if let Some(path) = config.ssl_key.as_deref() {
        url.query_pairs_mut().append_pair("ssl-key", path);
    }
    if let Some(path) = config.server_public_key_path.as_deref() {
        url.query_pairs_mut()
            .append_pair("server-public-key-path", path);
    }
    if config.enable_cleartext_plugin {
        url.query_pairs_mut()
            .append_pair("enable-cleartext-plugin", "true");
    }
    if config.transport != MySqlTransport::Tcp {
        url.query_pairs_mut()
            .append_pair("transport", config.transport.as_str());
        if let Some(path) = config.transport_path.as_deref() {
            url.query_pairs_mut().append_pair("transport-path", path);
        }
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
    let ssl_ca = ssl_mode.uses_ca().then(|| {
        url.query_pairs()
            .find_map(|(key, value)| (key == "ssl-ca").then(|| value.into_owned()))
    });
    let ssl_ca = ssl_ca.flatten();
    let ssl_cert = url
        .query_pairs()
        .find_map(|(key, value)| (key == "ssl-cert").then(|| value.into_owned()));
    let ssl_key = url
        .query_pairs()
        .find_map(|(key, value)| (key == "ssl-key").then(|| value.into_owned()));
    let server_public_key_path = url
        .query_pairs()
        .find_map(|(key, value)| (key == "server-public-key-path").then(|| value.into_owned()));
    let enable_cleartext_plugin = url
        .query_pairs()
        .find_map(|(key, value)| (key == "enable-cleartext-plugin").then(|| value.into_owned()))
        .map(|value| match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(DbOperationError::ConnectionFailed(
                "Invalid MySQL cleartext authentication setting".to_string(),
            )),
        })
        .transpose()?
        .unwrap_or(false);
    let transport = url
        .query_pairs()
        .find_map(|(key, value)| (key == "transport").then(|| value.parse()))
        .transpose()
        .map_err(|error: String| {
            DbOperationError::ConnectionFailed(format!("Invalid MySQL transport: {error}"))
        })?
        .unwrap_or_default();
    let transport_path = url
        .query_pairs()
        .find_map(|(key, value)| (key == "transport-path").then(|| value.into_owned()));

    Ok(MySqlDsn {
        transport,
        transport_path,
        host,
        port: url.port().unwrap_or(3306),
        database,
        username,
        password,
        ssl_mode,
        ssl_ca,
        ssl_cert,
        ssl_key,
        server_public_key_path,
        enable_cleartext_plugin,
    })
}

pub(super) fn parse_and_validate_mysql_dsn(dsn: &str) -> Result<MySqlDsn, DbOperationError> {
    let target = parse_mysql_dsn(dsn)?;
    validate_mysql_values(&target)?;
    validate_mysql_transport(&target)?;
    validate_mysql_tls_config(&target)?;
    Ok(target)
}

pub(super) fn map_mysql_tls_failure(
    error: DbOperationError,
    ssl_mode: MySqlSslMode,
) -> DbOperationError {
    match error {
        DbOperationError::QueryFailedAfterChange {
            source,
            refresh_scope,
        } => DbOperationError::QueryFailedAfterChange {
            source: Arc::new(map_mysql_tls_failure((*source).clone(), ssl_mode)),
            refresh_scope,
        },
        DbOperationError::ConnectionFailedWithKind {
            kind: ConnectionFailureKind::TlsCertificateVerification,
            details,
        } => {
            let kind = match ssl_mode {
                MySqlSslMode::VerifyCa => ConnectionFailureKind::TlsCaVerification,
                MySqlSslMode::VerifyIdentity => ConnectionFailureKind::TlsHostnameVerification,
                MySqlSslMode::Disabled | MySqlSslMode::Preferred | MySqlSslMode::Required => {
                    ConnectionFailureKind::TlsCertificateVerification
                }
            };
            DbOperationError::ConnectionFailedWithKind { kind, details }
        }
        error => error,
    }
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
        target.server_public_key_path.as_deref(),
        target.transport_path.as_deref(),
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

pub(super) fn validate_mysql_transport(target: &MySqlDsn) -> Result<(), DbOperationError> {
    if !target.transport.is_supported_on_current_platform() {
        return Err(DbOperationError::ConnectionFailed(
            "MySQL transport is not supported on this platform".to_string(),
        ));
    }
    match target.transport {
        MySqlTransport::Tcp => {
            if target.transport_path.is_some() {
                return Err(DbOperationError::ConnectionFailed(
                    "MySQL transport path is only valid for socket or named-pipe transport"
                        .to_string(),
                ));
            }
        }
        MySqlTransport::UnixSocket | MySqlTransport::NamedPipe => {
            if target
                .transport_path
                .as_deref()
                .is_none_or(|path| path.trim().is_empty())
            {
                return Err(DbOperationError::ConnectionFailed(
                    "MySQL transport path is required".to_string(),
                ));
            }
            if target.transport == MySqlTransport::NamedPipe
                && !matches!(
                    target.ssl_mode,
                    MySqlSslMode::Disabled | MySqlSslMode::Preferred
                )
            {
                return Err(DbOperationError::ConnectionFailed(
                    "MySQL named pipe transport does not support the selected TLS mode".to_string(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_mysql_tls_config(target: &MySqlDsn) -> Result<(), DbOperationError> {
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
    if target.enable_cleartext_plugin && !target.ssl_mode.allows_cleartext_auth() {
        return Err(DbOperationError::ConnectionFailed(
            "MySQL cleartext authentication requires REQUIRED, VERIFY_CA, or VERIFY_IDENTITY TLS mode"
                .to_string(),
        ));
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
    value
        .parse()
        .map_err(|_| DbOperationError::ConnectionFailed("Invalid MySQL TLS mode".to_string()))
}

fn decode_url_component(value: &str) -> Result<String, DbOperationError> {
    urlencoding::decode(value)
        .map(std::borrow::Cow::into_owned)
        .map_err(|error| DbOperationError::ConnectionFailed(format!("Invalid MySQL DSN: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RefreshScope;

    #[test]
    fn mysql_dsn_round_trip_preserves_connection_identity() {
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
        assert_eq!(parsed.database.as_deref(), config.database.as_deref());
        assert_eq!(parsed.username, "user name");
        assert_eq!(parsed.password, "p@ss#word");
        assert_eq!(parsed.ssl_mode, MySqlSslMode::Required);
    }

    #[test]
    fn rejects_unknown_ssl_mode_without_exposing_dsn_credentials() {
        let error =
            parse_mysql_dsn("mysql://user:secret@localhost:3306/app?ssl-mode=UNKNOWN").unwrap_err();

        assert!(matches!(
            error,
            DbOperationError::ConnectionFailed(details)
                if details == "Invalid MySQL TLS mode" && !details.contains("secret")
        ));
    }

    #[test]
    fn maps_ambiguous_tls_certificate_failures_from_the_parsed_ssl_mode() {
        for (ssl_mode, expected_kind) in [
            (
                MySqlSslMode::Preferred,
                ConnectionFailureKind::TlsCertificateVerification,
            ),
            (
                MySqlSslMode::Required,
                ConnectionFailureKind::TlsCertificateVerification,
            ),
            (
                MySqlSslMode::VerifyCa,
                ConnectionFailureKind::TlsCaVerification,
            ),
            (
                MySqlSslMode::VerifyIdentity,
                ConnectionFailureKind::TlsHostnameVerification,
            ),
        ] {
            let error = map_mysql_tls_failure(
                DbOperationError::ConnectionFailedWithKind {
                    kind: ConnectionFailureKind::TlsCertificateVerification,
                    details: "certificate verification failed".to_string(),
                },
                ssl_mode,
            );

            assert!(matches!(
                error,
                DbOperationError::ConnectionFailedWithKind { kind, .. } if kind == expected_kind
            ));
        }
    }

    #[test]
    fn leaves_non_tls_and_untyped_failures_unchanged() {
        let error = DbOperationError::ConnectionFailed("hostname mismatch".to_string());
        assert!(matches!(
            map_mysql_tls_failure(error, MySqlSslMode::VerifyIdentity),
            DbOperationError::ConnectionFailed(details) if details == "hostname mismatch"
        ));

        let error = DbOperationError::ConnectionFailedWithKind {
            kind: ConnectionFailureKind::TlsHandshake,
            details: "handshake failure".to_string(),
        };
        assert!(matches!(
            map_mysql_tls_failure(error, MySqlSslMode::VerifyCa),
            DbOperationError::ConnectionFailedWithKind {
                kind: ConnectionFailureKind::TlsHandshake,
                ..
            }
        ));
    }

    #[test]
    fn maps_wrapped_tls_failures_for_write_refreshes() {
        for (ssl_mode, expected_kind) in [
            (
                MySqlSslMode::VerifyCa,
                ConnectionFailureKind::TlsCaVerification,
            ),
            (
                MySqlSslMode::VerifyIdentity,
                ConnectionFailureKind::TlsHostnameVerification,
            ),
        ] {
            let error = DbOperationError::QueryFailedAfterChange {
                source: Arc::new(DbOperationError::ConnectionFailedWithKind {
                    kind: ConnectionFailureKind::TlsCertificateVerification,
                    details: "certificate verification failed".to_string(),
                }),
                refresh_scope: RefreshScope::Data,
            };

            let mapped = map_mysql_tls_failure(error, ssl_mode);
            match mapped {
                DbOperationError::QueryFailedAfterChange {
                    source,
                    refresh_scope,
                } => {
                    assert_eq!(refresh_scope, RefreshScope::Data);
                    assert!(matches!(
                        source.as_ref(),
                        DbOperationError::ConnectionFailedWithKind { kind, .. }
                            if *kind == expected_kind
                    ));
                }
                _ => panic!("TLS failure wrapper was not preserved"),
            }
        }
    }

    #[test]
    fn debug_redacts_mysql_passwords_and_preserves_connection_fields() {
        let cases = [
            (
                "mysql://user:secret@localhost:3306/app",
                "secret",
                "localhost",
                3306,
                Some("app"),
            ),
            (
                "mysql://user:p%40ss%23word@db.example:3307/analytics",
                "p@ss#word",
                "db.example",
                3307,
                Some("analytics"),
            ),
            (
                r"mysql://user:p%20a%23ss%3B%3D%22%5Cword@db.example:3308/reporting",
                r#"p a#ss;="\word"#,
                "db.example",
                3308,
                Some("reporting"),
            ),
        ];

        for (dsn, password, expected_host, expected_port, expected_database) in cases {
            let target = parse_mysql_dsn(dsn).unwrap();
            let debug = format!("{target:?}");
            let rendered_password = format!("{password:?}");

            assert!(!debug.contains(&rendered_password), "{debug}");
            assert!(
                debug.contains(&format!("host: {expected_host:?}")),
                "{debug}"
            );
            assert!(debug.contains(&format!("port: {expected_port}")), "{debug}");
            assert!(
                debug.contains(&format!("database: {expected_database:?}")),
                "{debug}"
            );
            assert!(debug.contains("password: \"****\""));
        }
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
    fn builds_and_parses_mysql_dsn_with_server_public_key_path() {
        let config = MySqlConnectionConfig::new(
            "db.example",
            3306,
            Some("app".to_string()),
            "user",
            "password",
            MySqlSslMode::Disabled,
        )
        .with_server_public_key_path(Some(r"C:\keys\server-public.pem".to_string()));

        let dsn = build_mysql_dsn(&config);
        assert!(dsn.contains("server-public-key-path="));
        let parsed = parse_mysql_dsn(&dsn).unwrap();

        assert_eq!(
            parsed.server_public_key_path.as_deref(),
            Some(r"C:\keys\server-public.pem")
        );
    }

    #[test]
    fn builds_and_parses_mysql_dsn_with_cleartext_auth_plugin() {
        let config = MySqlConnectionConfig::new(
            "db.example",
            3306,
            Some("app".to_string()),
            "user",
            "password",
            MySqlSslMode::Required,
        )
        .with_cleartext_auth_plugin(true);

        let dsn = build_mysql_dsn(&config);
        assert!(dsn.contains("enable-cleartext-plugin=true"));
        let parsed = parse_and_validate_mysql_dsn(&dsn).unwrap();

        assert!(parsed.enable_cleartext_plugin);
    }

    #[cfg(unix)]
    #[test]
    fn builds_and_parses_unix_socket_transport() {
        let config = MySqlConnectionConfig::new(
            "db example",
            0,
            Some("app".to_string()),
            "user",
            "password",
            MySqlSslMode::Disabled,
        )
        .with_transport(
            MySqlTransport::UnixSocket,
            Some("/run/mysqld/mysqld.sock".to_string()),
        );

        let dsn = build_mysql_dsn(&config);
        let parsed = parse_and_validate_mysql_dsn(&dsn).unwrap();

        assert_eq!(parsed.transport, MySqlTransport::UnixSocket);
        assert_eq!(parsed.host, "localhost");
        assert_eq!(parsed.port, 3306);
        assert_eq!(
            parsed.transport_path.as_deref(),
            Some("/run/mysqld/mysqld.sock")
        );
    }

    #[test]
    fn rejects_transport_path_for_tcp_dsn() {
        let error = parse_and_validate_mysql_dsn(
            "mysql://user:password@localhost:3306/app?transport=TCP&transport-path=%2Ftmp%2Fmysql.sock",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            DbOperationError::ConnectionFailed(details)
                if details == "MySQL transport path is only valid for socket or named-pipe transport"
        ));
    }

    #[test]
    fn rejects_cleartext_auth_plugin_without_required_tls() {
        let error = parse_and_validate_mysql_dsn(
            "mysql://user:password@localhost:3306/app?ssl-mode=PREFERRED&enable-cleartext-plugin=true",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            DbOperationError::ConnectionFailed(details)
                if details == "MySQL cleartext authentication requires REQUIRED, VERIFY_CA, or VERIFY_IDENTITY TLS mode"
        ));
    }

    #[test]
    fn ignores_ca_when_building_or_parsing_non_verification_dsn() {
        let config = MySqlConnectionConfig::new(
            "db.example",
            3307,
            Some("app".to_string()),
            "user",
            "password",
            MySqlSslMode::Required,
        );
        let dsn = format!("{}&ssl-ca=%2Fmissing%2Fca.pem", build_mysql_dsn(&config));
        let parsed = parse_mysql_dsn(&dsn).unwrap();

        assert_eq!(parsed.ssl_mode, MySqlSslMode::Required);
        assert_eq!(parsed.ssl_ca, None);
        assert!(!build_mysql_dsn(&config).contains("ssl-ca"));
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
    fn validates_tls_paths_without_accessing_files() {
        let target = parse_and_validate_mysql_dsn(
            "mysql://user:password@localhost:3306/app?ssl-mode=VERIFY_CA&ssl-ca=/missing/ca.pem&ssl-cert=/missing/client.pem&ssl-key=/missing/client-key.pem",
        )
        .unwrap();

        assert_eq!(target.ssl_mode, MySqlSslMode::VerifyCa);
        assert_eq!(target.ssl_ca.as_deref(), Some("/missing/ca.pem"));
    }

    #[test]
    fn rejects_control_characters_in_tls_paths() {
        for field in [
            "CA",
            "client certificate",
            "client key",
            "server public key",
        ] {
            let mut target = MySqlDsn {
                transport: MySqlTransport::Tcp,
                transport_path: None,
                host: "localhost".to_string(),
                port: 3306,
                database: None,
                username: "user".to_string(),
                password: "password".to_string(),
                ssl_mode: MySqlSslMode::Disabled,
                ssl_ca: None,
                ssl_cert: None,
                ssl_key: None,
                server_public_key_path: None,
                enable_cleartext_plugin: false,
            };
            match field {
                "CA" => target.ssl_ca = Some("ca\n.pem".to_string()),
                "client certificate" => target.ssl_cert = Some("client\r.pem".to_string()),
                "client key" => target.ssl_key = Some("client\0-key.pem".to_string()),
                "server public key" => {
                    target.server_public_key_path = Some("server\n-key.pem".to_string());
                }
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
}
