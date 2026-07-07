use crate::app::ports::outbound::DsnBuilder;
use crate::domain::connection::ConnectionProfile;

use super::PostgresAdapter;

impl PostgresAdapter {
    pub fn extract_database_name(dsn: &str) -> String {
        if let Some(name) = dsn.strip_prefix("service=") {
            return name.to_string();
        }
        if let Some(db) = find_conninfo_value(dsn, "dbname") {
            return decode_database_name(&db);
        }
        if let Some(db) = dsn
            .rsplit('/')
            .next()
            .map(|s| s.split('?').next().unwrap_or(s))
            .filter(|s| !s.is_empty() && !s.contains('='))
        {
            return decode_database_name(db);
        }
        "unknown".to_string()
    }
}

fn decode_database_name(name: &str) -> String {
    urlencoding::decode(name).map_or_else(|_| name.to_string(), std::borrow::Cow::into_owned)
}

impl DsnBuilder for PostgresAdapter {
    fn build_dsn(&self, profile: &ConnectionProfile) -> String {
        let config = profile
            .postgres_config()
            .expect("PostgresAdapter requires a PostgreSQL profile");
        let mut parts = Vec::new();
        push_conninfo_part(&mut parts, "host", config.host.trim());
        push_conninfo_part(&mut parts, "port", &config.port.to_string());
        push_conninfo_part(&mut parts, "dbname", config.database.trim());
        push_conninfo_part(&mut parts, "user", config.username.trim());
        push_conninfo_part(&mut parts, "password", config.password.as_str());
        push_conninfo_part(&mut parts, "sslmode", &config.ssl_mode.to_string());
        parts.join(" ")
    }
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

fn find_conninfo_value(dsn: &str, key: &str) -> Option<String> {
    let bytes = dsn.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let key_start = i;
        while bytes
            .get(i)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            i += 1;
        }
        let candidate = &dsn[key_start..i];
        if bytes.get(i) != Some(&b'=') {
            while bytes.get(i).is_some_and(|byte| !byte.is_ascii_whitespace()) {
                i += 1;
            }
            continue;
        }
        i += 1;
        let (value, next) = parse_conninfo_value(dsn, i);
        if candidate.eq_ignore_ascii_case(key) {
            return Some(value);
        }
        i = next;
    }
    None
}

fn parse_conninfo_value(dsn: &str, start: usize) -> (String, usize) {
    let bytes = dsn.as_bytes();
    if bytes.get(start) == Some(&b'\'') {
        let mut value = String::new();
        let mut i = start + 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' && bytes.get(i + 1).is_some() {
                let next = dsn[i + 1..].chars().next().unwrap();
                value.push(next);
                i += 1 + next.len_utf8();
            } else if bytes[i] == b'\'' {
                return (value, i + 1);
            } else {
                let ch = dsn[i..].chars().next().unwrap();
                value.push(ch);
                i += ch.len_utf8();
            }
        }
        (value, i)
    } else {
        let end = dsn[start..]
            .find(char::is_whitespace)
            .map_or(dsn.len(), |offset| start + offset);
        (dsn[start..end].to_string(), end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::connection::SslMode;

    fn make_test_profile() -> ConnectionProfile {
        ConnectionProfile::new_postgres(
            "Test Connection",
            "localhost",
            5432,
            "testdb",
            "testuser",
            "testpass",
            SslMode::Prefer,
        )
        .unwrap()
    }

    mod dsn_builder {
        use super::*;

        #[test]
        fn includes_all_connection_fields() {
            let adapter = PostgresAdapter::new();
            let profile = super::make_test_profile();
            let dsn = adapter.build_dsn(&profile);
            assert_eq!(
                dsn,
                "host='localhost' port='5432' dbname='testdb' user='testuser' password='testpass' sslmode='prefer'"
            );
        }

        #[test]
        fn quotes_special_chars() {
            let adapter = PostgresAdapter::new();
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
            let dsn = adapter.build_dsn(&profile);
            assert_eq!(
                dsn,
                "host='/var/run/postgresql' port='5432' dbname='my db' user='user\\'org' password='p\\\\ss word' sslmode='prefer'"
            );
        }

        #[test]
        fn omits_empty_userinfo_and_host() {
            let adapter = PostgresAdapter::new();
            let profile =
                ConnectionProfile::new_postgres("Test", "", 5432, "mydb", "", "", SslMode::Prefer)
                    .unwrap();

            let dsn = adapter.build_dsn(&profile);

            assert_eq!(dsn, "port='5432' dbname='mydb' sslmode='prefer'");
        }

        #[test]
        fn includes_password_without_user() {
            let adapter = PostgresAdapter::new();
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

            let dsn = adapter.build_dsn(&profile);

            assert_eq!(
                dsn,
                "port='5432' dbname='mydb' password='secret' sslmode='prefer'"
            );
        }
    }

    mod extract_database_name {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("postgres://user:pass@host:5432/mydb", "mydb")]
        #[case("postgres://localhost/testdb", "testdb")]
        #[case(
            "postgres://user:pass@host:5432/mydb?sslmode=prefer&connect_timeout=10",
            "mydb"
        )]
        fn uri_path_returns_dbname(#[case] dsn: &str, #[case] expected: &str) {
            assert_eq!(PostgresAdapter::extract_database_name(dsn), expected);
        }

        #[test]
        fn uri_path_decodes_percent_encoded_dbname() {
            assert_eq!(
                PostgresAdapter::extract_database_name("postgres://localhost/my%2Fdb"),
                "my/db"
            );
        }

        #[test]
        fn key_value_decodes_percent_encoded_dbname() {
            assert_eq!(
                PostgresAdapter::extract_database_name("host=localhost dbname=my%2Fdb"),
                "my/db"
            );
        }

        #[test]
        fn key_value_format() {
            assert_eq!(
                PostgresAdapter::extract_database_name("host=localhost dbname=mydb user=postgres"),
                "mydb"
            );
        }

        #[test]
        fn quoted_key_value_format() {
            assert_eq!(
                PostgresAdapter::extract_database_name(
                    "host='/var/run/postgresql' port='5432' dbname='test db' user='postgres'"
                ),
                "test db"
            );
        }

        #[test]
        fn empty_path() {
            assert_eq!(
                PostgresAdapter::extract_database_name("postgres://localhost/"),
                "unknown"
            );
        }

        #[test]
        fn key_value_only() {
            assert_eq!(
                PostgresAdapter::extract_database_name("host=localhost user=postgres"),
                "unknown"
            );
        }

        #[test]
        fn service_dsn_returns_service_name() {
            assert_eq!(
                PostgresAdapter::extract_database_name("service=mydb"),
                "mydb"
            );
        }

        #[test]
        fn roundtrip_build_then_extract_returns_original_dbname() {
            let adapter = PostgresAdapter::new();
            let profile = ConnectionProfile::new_postgres(
                "Test",
                "localhost",
                5432,
                "my/db",
                "testuser",
                "testpass",
                SslMode::Prefer,
            )
            .unwrap();
            let dsn = adapter.build_dsn(&profile);

            assert_eq!(PostgresAdapter::extract_database_name(&dsn), "my/db");
        }
    }
}
