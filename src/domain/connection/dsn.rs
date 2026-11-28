use url::Url;

use super::{
    ConnectionEndpoint, MySqlConnectionConfig, MySqlTransport, PostgresConnectionConfig,
    PostgresEndpoint, SqliteConnectionConfig,
};

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every engine must specify its connection string."
)]
pub fn build_dsn(endpoint: &ConnectionEndpoint) -> String {
    match endpoint {
        ConnectionEndpoint::PostgreSQL(endpoint) => postgres_conninfo(endpoint),
        ConnectionEndpoint::SQLite(config) => sqlite_dsn(config),
        ConnectionEndpoint::MySQL(config) => mysql_dsn(config),
    }
}

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every PostgreSQL endpoint must specify its connection string."
)]
pub fn postgres_conninfo(endpoint: &PostgresEndpoint) -> String {
    match endpoint {
        PostgresEndpoint::Settings(config) => settings_conninfo(config),
        PostgresEndpoint::Service(service_name) => format!("service={service_name}"),
    }
}

fn settings_conninfo(config: &PostgresConnectionConfig) -> String {
    let mut parts = Vec::new();
    push_conninfo_part(&mut parts, "host", config.host.trim());
    push_conninfo_part(&mut parts, "port", &config.port.to_string());
    push_conninfo_part(&mut parts, "dbname", config.database.trim());
    push_conninfo_part(&mut parts, "user", config.username.trim());
    push_conninfo_part(&mut parts, "password", config.password.as_str());
    push_conninfo_part(&mut parts, "sslmode", &config.ssl_mode.to_string());
    parts.join(" ")
}

fn push_conninfo_part(parts: &mut Vec<String>, key: &str, value: &str) {
    if !value.is_empty() {
        parts.push(format!("{key}={}", quote_conninfo_value(value)));
    }
}

fn quote_conninfo_value(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

fn sqlite_dsn(config: &SqliteConnectionConfig) -> String {
    format!("sqlite://{}", config.path())
}

fn mysql_dsn(config: &MySqlConnectionConfig) -> String {
    let mut url = Url::parse("mysql://localhost").expect("static MySQL URL is valid");
    url.set_username(&config.username.replace('%', "%25"))
        .expect("MySQL username is valid URL data");
    url.set_password(Some(&config.password.replace('%', "%25")))
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

pub fn normalize_mysql_host(host: &str) -> String {
    host.strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{ConnectionProfile, SslMode};

    mod postgres_conninfo {
        use super::*;

        #[test]
        fn quotes_special_chars() {
            let profile = ConnectionProfile::new_postgres(
                "Test",
                "/var/run/postgresql",
                5432,
                "my db",
                "user'org",
                "p\\ss word",
                SslMode::Prefer,
            )
            .unwrap();
            let dsn = postgres_conninfo(&PostgresEndpoint::Settings(
                profile.config.as_postgres().unwrap().clone(),
            ));
            assert_eq!(
                dsn,
                "host='/var/run/postgresql' port='5432' dbname='my db' user='user\\'org' password='p\\\\ss word' sslmode='prefer'"
            );
        }

        #[test]
        fn omits_empty_userinfo_and_host() {
            let profile =
                ConnectionProfile::new_postgres("Test", "", 5432, "mydb", "", "", SslMode::Prefer)
                    .unwrap();

            let dsn = postgres_conninfo(&PostgresEndpoint::Settings(
                profile.config.as_postgres().unwrap().clone(),
            ));

            assert_eq!(dsn, "port='5432' dbname='mydb' sslmode='prefer'");
        }

        #[test]
        fn includes_password_without_user() {
            let profile = ConnectionProfile::new_postgres(
                "Test",
                "",
                5432,
                "mydb",
                "",
                "secret",
                SslMode::Prefer,
            )
            .unwrap();

            let dsn = postgres_conninfo(&PostgresEndpoint::Settings(
                profile.config.as_postgres().unwrap().clone(),
            ));

            assert_eq!(
                dsn,
                "port='5432' dbname='mydb' password='secret' sslmode='prefer'"
            );
        }
    }

    mod mysql_dsn {
        use super::*;
        use crate::connection::MySqlSslMode;

        #[test]
        fn mysql_preview_includes_server_public_key_path() {
            let config = MySqlConnectionConfig::new(
                "db.example",
                3306,
                Some("app".to_string()),
                "user",
                "password",
                MySqlSslMode::Disabled,
            )
            .with_server_public_key_path(Some(r"C:\keys\server-public.pem".to_string()));

            let dsn = mysql_dsn(&config);
            assert!(dsn.contains("server-public-key-path="));
        }

        #[test]
        fn mysql_preview_includes_cleartext_auth_plugin() {
            let config = MySqlConnectionConfig::new(
                "db.example",
                3306,
                Some("app".to_string()),
                "user",
                "password",
                MySqlSslMode::Required,
            )
            .with_cleartext_auth_plugin(true);

            let dsn = mysql_dsn(&config);
            assert!(dsn.contains("enable-cleartext-plugin=true"));
        }

        #[test]
        fn preview_url_encodes_percent_signs_in_credentials() {
            let profile = ConnectionProfile::new_mysql(
                "Preview",
                "db.example",
                3307,
                Some("app".to_string()),
                "us er%41",
                "sec%2Fret",
                MySqlSslMode::Preferred,
            )
            .unwrap();

            assert_eq!(
                mysql_dsn(profile.config.as_mysql().unwrap()),
                "mysql://us%20er%2541:sec%252Fret@db.example:3307/app?ssl-mode=PREFERRED"
            );
        }
    }
}
