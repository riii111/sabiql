use crate::app::ports::outbound::DsnBuilder;
use crate::domain::connection::ConnectionProfile;

use super::PostgresAdapter;

impl PostgresAdapter {
    pub(super) fn extract_database_name(dsn: &str) -> String {
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

pub(super) fn quote_conninfo_value(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

fn find_conninfo_value(dsn: &str, key: &str) -> Option<String> {
    find_conninfo_part(dsn, key).map(|(value, _)| value)
}

// Keep credentials out of child argv for generated conninfo and PostgreSQL URIs.
pub(super) fn take_explicit_password(dsn: &str) -> Result<Option<(String, String)>, &'static str> {
    if let Some((password, range)) = find_conninfo_part(dsn, "password") {
        if password.is_empty() {
            return Ok(None);
        }
        let mut connection = dsn.to_string();
        // An explicit empty password suppresses service/environment defaults while allowing passfile.
        connection.replace_range(range, "password=''");
        return Ok(Some((connection, password)));
    }
    take_uri_password(dsn)
}

fn take_uri_password(dsn: &str) -> Result<Option<(String, String)>, &'static str> {
    if !(dsn.starts_with("postgres://") || dsn.starts_with("postgresql://")) {
        return Ok(None);
    }

    let Some(scheme_end) = dsn.find("://") else {
        return Ok(None);
    };
    let authority_start = scheme_end + 3;
    let authority_end = dsn[authority_start..]
        .find(['/', '?', '#'])
        .map_or(dsn.len(), |offset| authority_start + offset);
    let authority = &dsn[authority_start..authority_end];
    let mut password = None;
    let mut ranges = Vec::new();

    if let Some(at_offset) = authority.rfind('@') {
        let userinfo = &authority[..at_offset];
        if let Some(colon_offset) = userinfo.find(':') {
            let password_start = authority_start + colon_offset + 1;
            let password_end = authority_start + at_offset;
            let encoded_password = &dsn[password_start..password_end];
            if !encoded_password.is_empty() {
                password = Some(
                    decode_uri_component(encoded_password)
                        .map_err(|()| "Invalid PostgreSQL URI password encoding")?,
                );
                ranges.push((authority_start + colon_offset, password_end));
            }
        }
    }

    if let Some(query_offset) = dsn[authority_end..].find('?') {
        let query_start = authority_end + query_offset + 1;
        let query_end = dsn[query_start..]
            .find('#')
            .map_or(dsn.len(), |offset| query_start + offset);
        let query = &dsn[query_start..query_end];
        let mut segment_start = query_start;
        for segment in query.split('&') {
            let segment_end = segment_start + segment.len();
            if let Some(equal_offset) = segment.find('=') {
                let key = decode_uri_component(&segment[..equal_offset])
                    .map_err(|()| "Invalid PostgreSQL URI parameter encoding")?;
                let value_start = segment_start + equal_offset + 1;
                let encoded_value = &dsn[value_start..segment_end];
                if key.eq_ignore_ascii_case("sslpassword") {
                    if !encoded_value.is_empty()
                        && !decode_uri_component(encoded_value)
                            .map_err(|()| "Invalid PostgreSQL URI password encoding")?
                            .is_empty()
                    {
                        return Err("PostgreSQL URI sslpassword cannot be passed securely to psql");
                    }
                } else if key.eq_ignore_ascii_case("password") && !encoded_value.is_empty() {
                    let decoded_password = decode_uri_component(encoded_value)
                        .map_err(|()| "Invalid PostgreSQL URI password encoding")?;
                    if !decoded_password.is_empty() {
                        // libpq applies later non-empty URI keywords last.
                        password = Some(decoded_password);
                    }
                    ranges.push((value_start, segment_end));
                }
            }
            segment_start = segment_end.saturating_add(1);
        }
    }

    let Some(password) = password else {
        return Ok(None);
    };
    let mut connection = dsn.to_string();
    for (start, end) in ranges.into_iter().rev() {
        connection.replace_range(start..end, "");
    }
    Ok(Some((connection, password)))
}

fn decode_uri_component(value: &str) -> Result<String, ()> {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if bytes
                .get(i + 1)
                .is_none_or(|byte| !byte.is_ascii_hexdigit())
                || bytes
                    .get(i + 2)
                    .is_none_or(|byte| !byte.is_ascii_hexdigit())
            {
                return Err(());
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    urlencoding::decode(value)
        .map(std::borrow::Cow::into_owned)
        .map_err(|_| ())
}

fn find_conninfo_part(dsn: &str, key: &str) -> Option<(String, std::ops::Range<usize>)> {
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
            return Some((value, key_start..next));
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
