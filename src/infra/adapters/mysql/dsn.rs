use std::{fmt, sync::Arc};

use crate::app::ports::outbound::{ConnectionFailureKind, DbOperationError};
use crate::domain::connection::{
    MySqlConnectionConfig, MySqlSslMode, MySqlTransport, normalize_mysql_host,
};

pub(super) struct NormalizedMySqlConfig {
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

impl NormalizedMySqlConfig {
    pub(super) fn new(config: &MySqlConnectionConfig) -> Result<Self, DbOperationError> {
        let (host, port, transport_path) = match config.transport {
            MySqlTransport::Tcp => (normalize_mysql_host(&config.host), config.port, None),
            MySqlTransport::UnixSocket | MySqlTransport::NamedPipe => {
                ("localhost".to_string(), 3306, config.transport_path.clone())
            }
        };
        let target = Self {
            transport: config.transport,
            transport_path,
            host,
            port,
            database: config
                .database
                .clone()
                .filter(|database| !database.is_empty()),
            username: config.username.clone(),
            password: config.password.clone(),
            ssl_mode: config.ssl_mode,
            ssl_ca: if config.ssl_mode.uses_ca() {
                config.ssl_ca.clone()
            } else {
                None
            },
            ssl_cert: config.ssl_cert.clone(),
            ssl_key: config.ssl_key.clone(),
            server_public_key_path: config.server_public_key_path.clone(),
            enable_cleartext_plugin: config.enable_cleartext_plugin,
        };
        validate_mysql_values(&target)?;
        validate_mysql_transport(&target)?;
        validate_mysql_tls_config(&target)?;
        Ok(target)
    }
}

impl fmt::Debug for NormalizedMySqlConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NormalizedMySqlConfig")
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

pub(super) fn validate_mysql_values(
    target: &NormalizedMySqlConfig,
) -> Result<(), DbOperationError> {
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

pub(super) fn validate_mysql_transport(
    target: &NormalizedMySqlConfig,
) -> Result<(), DbOperationError> {
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
            if !target.transport.allows_ssl_mode(target.ssl_mode) {
                return Err(DbOperationError::ConnectionFailed(
                    "MySQL named pipe transport does not support the selected TLS mode".to_string(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_mysql_tls_config(
    target: &NormalizedMySqlConfig,
) -> Result<(), DbOperationError> {
    let has_tls_path =
        target.ssl_ca.is_some() || target.ssl_cert.is_some() || target.ssl_key.is_some();
    if target.ssl_mode == MySqlSslMode::Disabled && has_tls_path {
        return Err(DbOperationError::ConnectionFailed(
            "MySQL TLS paths require an enabled TLS mode".to_string(),
        ));
    }
    if target.ssl_mode.uses_ca() && target.ssl_ca.is_none() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RefreshScope;
    use crate::domain::connection::{ConnectionEndpoint, build_dsn};

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
        let parsed = NormalizedMySqlConfig::new(&config).unwrap();

        assert_eq!(parsed.host, "db.example");
        assert_eq!(parsed.port, 3307);
        assert_eq!(parsed.database.as_deref(), config.database.as_deref());
        assert_eq!(parsed.username, "user name");
        assert_eq!(parsed.password, "p@ss#word");
        assert_eq!(parsed.ssl_mode, MySqlSslMode::Required);
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
    fn debug_redacts_mysql_password_and_preserves_connection_fields() {
        let password = r#"p a#ss;="\word"#;
        let config = MySqlConnectionConfig::new(
            "db.example",
            3308,
            Some("reporting".to_string()),
            "user",
            password,
            MySqlSslMode::Preferred,
        );
        let target = NormalizedMySqlConfig::new(&config).unwrap();
        let debug = format!("{target:?}");

        assert!(!debug.contains(&format!("{password:?}")), "{debug}");
        assert!(debug.contains("host: \"db.example\""), "{debug}");
        assert!(debug.contains("port: 3308"), "{debug}");
        assert!(debug.contains("database: Some(\"reporting\")"), "{debug}");
        assert!(debug.contains("password: \"****\""), "{debug}");
    }

    #[test]
    fn rejects_cleartext_auth_plugin_without_required_tls() {
        let config = MySqlConnectionConfig::new(
            "localhost",
            3306,
            Some("app".to_string()),
            "user",
            "password",
            MySqlSslMode::Preferred,
        )
        .with_cleartext_auth_plugin(true);
        let error = NormalizedMySqlConfig::new(&config).unwrap_err();

        assert!(matches!(
            error,
            DbOperationError::ConnectionFailed(details)
                if details == "MySQL cleartext authentication requires REQUIRED, VERIFY_CA, or VERIFY_IDENTITY TLS mode"
        ));
    }

    #[test]
    fn ignores_ca_when_building_or_parsing_non_verification_dsn() {
        let mut config = MySqlConnectionConfig::new(
            "db.example",
            3307,
            Some("app".to_string()),
            "user",
            "password",
            MySqlSslMode::Required,
        );
        config.ssl_ca = Some("/missing/ca.pem".to_string());
        let parsed = NormalizedMySqlConfig::new(&config).unwrap();

        assert_eq!(parsed.ssl_mode, MySqlSslMode::Required);
        assert_eq!(parsed.ssl_ca, None);
        assert!(!build_dsn(&ConnectionEndpoint::MySQL(config)).contains("ssl-ca"));
    }

    #[test]
    fn rejects_control_characters_in_tls_paths() {
        for field in [
            "CA",
            "client certificate",
            "client key",
            "server public key",
        ] {
            let mut target = NormalizedMySqlConfig {
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

    mod connection_values {
        use super::*;

        #[derive(Debug, PartialEq, Eq)]
        struct NormalizedValues {
            transport: MySqlTransport,
            transport_path: Option<String>,
            host: String,
            port: u16,
            database: Option<String>,
            username: String,
            password: String,
            ssl_mode: MySqlSslMode,
            ssl_ca: Option<String>,
            ssl_cert: Option<String>,
            ssl_key: Option<String>,
            server_public_key_path: Option<String>,
            enable_cleartext_plugin: bool,
        }

        fn values_passed_to_cli(
            config: &MySqlConnectionConfig,
        ) -> Result<NormalizedValues, String> {
            match NormalizedMySqlConfig::new(config) {
                Ok(target) => Ok(NormalizedValues {
                    transport: target.transport,
                    transport_path: target.transport_path,
                    host: target.host,
                    port: target.port,
                    database: target.database,
                    username: target.username,
                    password: target.password,
                    ssl_mode: target.ssl_mode,
                    ssl_ca: target.ssl_ca,
                    ssl_cert: target.ssl_cert,
                    ssl_key: target.ssl_key,
                    server_public_key_path: target.server_public_key_path,
                    enable_cleartext_plugin: target.enable_cleartext_plugin,
                }),
                Err(DbOperationError::ConnectionFailed(details)) => Err(details),
                Err(error) => panic!("unexpected MySQL connection value error: {error:?}"),
            }
        }

        fn tcp_config() -> MySqlConnectionConfig {
            MySqlConnectionConfig::new(
                "db.example",
                3307,
                Some("app".to_string()),
                "user",
                "secret",
                MySqlSslMode::Preferred,
            )
        }

        fn tcp_values() -> NormalizedValues {
            NormalizedValues {
                transport: MySqlTransport::Tcp,
                transport_path: None,
                host: "db.example".to_string(),
                port: 3307,
                database: Some("app".to_string()),
                username: "user".to_string(),
                password: "secret".to_string(),
                ssl_mode: MySqlSslMode::Preferred,
                ssl_ca: None,
                ssl_cert: None,
                ssl_key: None,
                server_public_key_path: None,
                enable_cleartext_plugin: false,
            }
        }

        mod credentials {
            use super::*;

            #[test]
            fn percent_sequences_are_preserved() {
                let mut config = tcp_config();
                config.username = "us er%41".to_string();
                config.password = "A%2Fb".to_string();

                assert_eq!(
                    values_passed_to_cli(&config),
                    Ok(NormalizedValues {
                        username: "us er%41".to_string(),
                        password: "A%2Fb".to_string(),
                        ..tcp_values()
                    })
                );
            }

            #[test]
            fn invalid_utf8_percent_sequences_are_preserved() {
                let mut username = tcp_config();
                username.username = "abc%FF".to_string();
                let mut password = tcp_config();
                password.password = "abc%FF".to_string();

                assert_eq!(
                    values_passed_to_cli(&username),
                    Ok(NormalizedValues {
                        username: "abc%FF".to_string(),
                        ..tcp_values()
                    })
                );
                assert_eq!(
                    values_passed_to_cli(&password),
                    Ok(NormalizedValues {
                        password: "abc%FF".to_string(),
                        ..tcp_values()
                    })
                );
            }

            #[test]
            fn control_characters_are_rejected() {
                let mut config = tcp_config();
                config.password = "pa\nss".to_string();

                assert_eq!(
                    values_passed_to_cli(&config),
                    Err("MySQL connection settings contain a control character".to_string())
                );
            }
        }

        mod tcp_host {
            use super::*;

            fn values_for_host(host: &str) -> Result<NormalizedValues, String> {
                let mut config = tcp_config();
                config.host = host.to_string();
                values_passed_to_cli(&config)
            }

            #[test]
            fn non_ascii_host_is_preserved() {
                assert_eq!(
                    values_for_host("ホスト"),
                    Ok(NormalizedValues {
                        host: "ホスト".to_string(),
                        ..tcp_values()
                    })
                );
            }

            #[test]
            fn noncanonical_uppercase_ipv6_host_is_preserved() {
                let host = "FE80:0:0:0:0:0:0:1";

                assert_eq!(
                    values_for_host(host),
                    Ok(NormalizedValues {
                        host: host.to_string(),
                        ..tcp_values()
                    })
                );
            }

            #[test]
            fn bracketed_ipv6_host_loses_its_brackets() {
                assert_eq!(
                    values_for_host("[::1]"),
                    Ok(NormalizedValues {
                        host: "::1".to_string(),
                        ..tcp_values()
                    })
                );
            }
        }

        mod database {
            use super::*;
            use rstest::rstest;

            #[rstest]
            #[case::current(".")]
            #[case::parent("..")]
            fn dot_segment_names_are_preserved(#[case] database: &str) {
                let mut config = tcp_config();
                config.database = Some(database.to_string());

                assert_eq!(
                    values_passed_to_cli(&config),
                    Ok(NormalizedValues {
                        database: Some(database.to_string()),
                        ..tcp_values()
                    })
                );
            }

            #[test]
            fn empty_name_is_omitted() {
                let mut config = tcp_config();
                config.database = Some(String::new());

                assert_eq!(
                    values_passed_to_cli(&config),
                    Ok(NormalizedValues {
                        database: None,
                        ..tcp_values()
                    })
                );
            }
        }

        mod transport {
            use super::*;

            #[test]
            fn tcp_drops_a_rewritten_transport_path() {
                let mut config = tcp_config();
                config.transport_path = Some("/tmp/mysql.sock".to_string());

                assert_eq!(values_passed_to_cli(&config), Ok(tcp_values()));
            }

            #[cfg(unix)]
            #[test]
            fn unix_socket_replaces_tcp_host_and_port_with_defaults() {
                let config = MySqlConnectionConfig {
                    host: "db example".to_string(),
                    port: 0,
                    ..tcp_config()
                }
                .with_transport(
                    MySqlTransport::UnixSocket,
                    Some("/run/mysqld/mysqld.sock".to_string()),
                );

                assert_eq!(
                    values_passed_to_cli(&config),
                    Ok(NormalizedValues {
                        transport: MySqlTransport::UnixSocket,
                        transport_path: Some("/run/mysqld/mysqld.sock".to_string()),
                        host: "localhost".to_string(),
                        port: 3306,
                        ..tcp_values()
                    })
                );
            }

            #[cfg(not(unix))]
            #[test]
            fn unix_socket_is_unsupported_on_this_platform() {
                let config = tcp_config().with_transport(
                    MySqlTransport::UnixSocket,
                    Some("/run/mysqld/mysqld.sock".to_string()),
                );

                assert_eq!(
                    values_passed_to_cli(&config),
                    Err("MySQL transport is not supported on this platform".to_string())
                );
            }

            #[cfg(windows)]
            #[test]
            fn named_pipe_replaces_tcp_host_and_port_with_defaults() {
                let config = MySqlConnectionConfig {
                    host: "db example".to_string(),
                    port: 0,
                    ..tcp_config()
                }
                .with_transport(
                    MySqlTransport::NamedPipe,
                    Some(r"\\.\pipe\MySQL".to_string()),
                );

                assert_eq!(
                    values_passed_to_cli(&config),
                    Ok(NormalizedValues {
                        transport: MySqlTransport::NamedPipe,
                        transport_path: Some(r"\\.\pipe\MySQL".to_string()),
                        host: "localhost".to_string(),
                        port: 3306,
                        ..tcp_values()
                    })
                );
            }

            #[cfg(windows)]
            #[test]
            fn named_pipe_rejects_tls_modes_beyond_preferred() {
                let config = MySqlConnectionConfig {
                    ssl_mode: MySqlSslMode::Required,
                    ..tcp_config()
                }
                .with_transport(
                    MySqlTransport::NamedPipe,
                    Some(r"\\.\pipe\MySQL".to_string()),
                );

                assert_eq!(
                    values_passed_to_cli(&config),
                    Err(
                        "MySQL named pipe transport does not support the selected TLS mode"
                            .to_string()
                    )
                );
            }

            #[cfg(not(windows))]
            #[test]
            fn named_pipe_is_unsupported_on_this_platform() {
                let config = tcp_config().with_transport(
                    MySqlTransport::NamedPipe,
                    Some(r"\\.\pipe\MySQL".to_string()),
                );

                assert_eq!(
                    values_passed_to_cli(&config),
                    Err("MySQL transport is not supported on this platform".to_string())
                );
            }
        }

        mod tls {
            use super::*;
            use rstest::rstest;

            fn config_with_mode(ssl_mode: MySqlSslMode) -> MySqlConnectionConfig {
                MySqlConnectionConfig {
                    ssl_mode,
                    ..tcp_config()
                }
            }

            #[rstest]
            #[case::disabled(MySqlSslMode::Disabled)]
            #[case::preferred(MySqlSslMode::Preferred)]
            #[case::required(MySqlSslMode::Required)]
            fn modes_without_ca_verification_need_no_paths(#[case] ssl_mode: MySqlSslMode) {
                assert_eq!(
                    values_passed_to_cli(&config_with_mode(ssl_mode)),
                    Ok(NormalizedValues {
                        ssl_mode,
                        ..tcp_values()
                    })
                );
            }

            #[rstest]
            #[case::verify_ca(MySqlSslMode::VerifyCa)]
            #[case::verify_identity(MySqlSslMode::VerifyIdentity)]
            fn ca_verification_modes_require_a_ca_path(#[case] ssl_mode: MySqlSslMode) {
                let with_ca = config_with_mode(ssl_mode).with_tls_paths(
                    Some("/certs/ca.pem".to_string()),
                    None,
                    None,
                );

                assert_eq!(
                    values_passed_to_cli(&config_with_mode(ssl_mode)),
                    Err("MySQL CA path is required for certificate verification".to_string())
                );
                assert_eq!(
                    values_passed_to_cli(&with_ca),
                    Ok(NormalizedValues {
                        ssl_mode,
                        ssl_ca: Some("/certs/ca.pem".to_string()),
                        ..tcp_values()
                    })
                );
            }

            #[test]
            fn ca_path_left_outside_ca_verification_is_dropped() {
                let mut config = config_with_mode(MySqlSslMode::Required);
                config.ssl_ca = Some("/certs/ca.pem".to_string());

                assert_eq!(
                    values_passed_to_cli(&config),
                    Ok(NormalizedValues {
                        ssl_mode: MySqlSslMode::Required,
                        ..tcp_values()
                    })
                );
            }

            #[test]
            fn client_certificate_without_key_is_rejected() {
                let config = config_with_mode(MySqlSslMode::Required).with_tls_paths(
                    None,
                    Some("/certs/client.pem".to_string()),
                    None,
                );

                assert_eq!(
                    values_passed_to_cli(&config),
                    Err("MySQL client certificate and key must be specified together".to_string())
                );
            }

            #[test]
            fn client_paths_with_disabled_tls_are_rejected() {
                let config = config_with_mode(MySqlSslMode::Disabled).with_tls_paths(
                    None,
                    Some("/certs/client.pem".to_string()),
                    Some("/certs/client-key.pem".to_string()),
                );

                assert_eq!(
                    values_passed_to_cli(&config),
                    Err("MySQL TLS paths require an enabled TLS mode".to_string())
                );
            }
        }
    }
}
