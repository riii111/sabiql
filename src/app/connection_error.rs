use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionErrorKind {
    PsqlNotFound,
    HostUnreachable,
    AuthFailed,
    DatabaseNotFound,
    Timeout,
    #[default]
    Unknown,
}

impl ConnectionErrorKind {
    pub fn classify(stderr: &str) -> Self {
        let stderr_lower = stderr.to_lowercase();

        if stderr_lower.contains("command not found")
            || stderr_lower.contains("not found: psql")
            || stderr_lower.contains("not recognized")
        {
            return Self::PsqlNotFound;
        }

        if stderr_lower.contains("could not translate host name")
            || stderr_lower.contains("name or service not known")
            || stderr_lower.contains("nodename nor servname provided")
            || stderr_lower.contains("no such host")
        {
            return Self::HostUnreachable;
        }

        if stderr_lower.contains("password authentication failed")
            || stderr_lower.contains("authentication failed")
            || (stderr_lower.contains("fatal:") && stderr_lower.contains("password"))
        {
            return Self::AuthFailed;
        }

        if stderr_lower.contains("does not exist")
            && (stderr_lower.contains("database") || stderr_lower.contains("fatal:"))
        {
            return Self::DatabaseNotFound;
        }

        if stderr_lower.contains("timeout expired")
            || stderr_lower.contains("timed out")
            || stderr_lower.contains("connection timed out")
        {
            return Self::Timeout;
        }

        Self::Unknown
    }

    pub fn summary(&self) -> &'static str {
        match self {
            Self::PsqlNotFound => "psql command not found",
            Self::HostUnreachable => "Could not resolve host",
            Self::AuthFailed => "Authentication failed",
            Self::DatabaseNotFound => "Database does not exist",
            Self::Timeout => "Connection timed out",
            Self::Unknown => "Connection failed",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::PsqlNotFound => "Install PostgreSQL or add psql to PATH",
            Self::HostUnreachable => "Check the hostname",
            Self::AuthFailed => "Check username and password",
            Self::DatabaseNotFound => "Check database name",
            Self::Timeout => "Check network connectivity",
            Self::Unknown => "See details for more information",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionErrorInfo {
    pub kind: ConnectionErrorKind,
    pub raw_details: String,
    pub masked_details: String,
}

impl ConnectionErrorInfo {
    pub fn new(raw_stderr: impl Into<String>) -> Self {
        let raw_details = raw_stderr.into();
        let kind = ConnectionErrorKind::classify(&raw_details);
        let masked_details = Self::mask_password(&raw_details);

        Self {
            kind,
            raw_details,
            masked_details,
        }
    }

    pub fn with_kind(kind: ConnectionErrorKind, raw_stderr: impl Into<String>) -> Self {
        let raw_details = raw_stderr.into();
        let masked_details = Self::mask_password(&raw_details);

        Self {
            kind,
            raw_details,
            masked_details,
        }
    }

    pub fn summary(&self) -> &'static str {
        self.kind.summary()
    }

    pub fn hint(&self) -> &'static str {
        self.kind.hint()
    }

    fn mask_password(text: &str) -> String {
        // postgres://user:password@host -> postgres://user:****@host
        let url_re = Regex::new(r"(postgres://[^:]+:)[^@]+(@)").unwrap();
        let result = url_re.replace_all(text, "${1}****${2}");

        // password=xxx -> password=****
        let param_re = Regex::new(r"(password=)[^\s]+").unwrap();
        let result = param_re.replace_all(&result, "${1}****");

        // PGPASSWORD=xxx -> PGPASSWORD=****
        let env_re = Regex::new(r"(PGPASSWORD=)[^\s]+").unwrap();
        env_re.replace_all(&result, "${1}****").into_owned()
    }
}

impl Default for ConnectionErrorInfo {
    fn default() -> Self {
        Self {
            kind: ConnectionErrorKind::Unknown,
            raw_details: String::new(),
            masked_details: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod error_kind_classification {
        use super::*;

        #[test]
        fn classifies_psql_not_found_bash() {
            let stderr = "psql: command not found";
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::PsqlNotFound
            );
        }

        #[test]
        fn classifies_psql_not_found_sh() {
            let stderr = "/bin/sh: psql: command not found";
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::PsqlNotFound
            );
        }

        #[test]
        fn classifies_psql_not_found_zsh() {
            let stderr = "zsh: command not found: psql";
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::PsqlNotFound
            );
        }

        #[test]
        fn classifies_host_unreachable_macos() {
            let stderr = r#"psql: error: could not translate host name "nonexistent.host" to address: nodename nor servname provided, or not known"#;
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::HostUnreachable
            );
        }

        #[test]
        fn classifies_host_unreachable_linux() {
            let stderr = r#"psql: error: could not translate host name "badhost" to address: Name or service not known"#;
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::HostUnreachable
            );
        }

        #[test]
        fn classifies_auth_failed_ipv4() {
            let stderr = r#"psql: error: connection to server at "localhost" (127.0.0.1), port 5432 failed: FATAL:  password authentication failed for user "wronguser""#;
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::AuthFailed
            );
        }

        #[test]
        fn classifies_auth_failed_ipv6() {
            let stderr = r#"psql: error: connection to server at "localhost" (::1), port 5432 failed: FATAL:  password authentication failed for user "postgres""#;
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::AuthFailed
            );
        }

        #[test]
        fn classifies_database_not_found() {
            let stderr = r#"psql: error: connection to server at "localhost" (127.0.0.1), port 5432 failed: FATAL:  database "nonexistent_db" does not exist"#;
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::DatabaseNotFound
            );
        }

        #[test]
        fn classifies_timeout_expired() {
            let stderr = r#"psql: error: connection to server at "192.168.1.100", port 5432 failed: timeout expired"#;
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Timeout
            );
        }

        #[test]
        fn classifies_connection_timed_out() {
            let stderr = r#"psql: error: connection to server at "10.0.0.1", port 5432 failed: Connection timed out"#;
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Timeout
            );
        }

        #[test]
        fn classifies_unknown_for_connection_refused() {
            let stderr = r#"psql: error: connection to server at "localhost" (127.0.0.1), port 5432 failed: Connection refused"#;
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Unknown
            );
        }

        #[test]
        fn classifies_unknown_for_empty_string() {
            assert_eq!(
                ConnectionErrorKind::classify(""),
                ConnectionErrorKind::Unknown
            );
        }

        #[test]
        fn classifies_unknown_for_arbitrary_error() {
            let stderr = "Some random error message";
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Unknown
            );
        }
    }

    mod error_kind_messages {
        use super::*;

        #[test]
        fn summary_messages_are_not_empty() {
            let kinds = [
                ConnectionErrorKind::PsqlNotFound,
                ConnectionErrorKind::HostUnreachable,
                ConnectionErrorKind::AuthFailed,
                ConnectionErrorKind::DatabaseNotFound,
                ConnectionErrorKind::Timeout,
                ConnectionErrorKind::Unknown,
            ];

            for kind in kinds {
                assert!(
                    !kind.summary().is_empty(),
                    "Summary for {:?} is empty",
                    kind
                );
            }
        }

        #[test]
        fn hint_messages_are_not_empty() {
            let kinds = [
                ConnectionErrorKind::PsqlNotFound,
                ConnectionErrorKind::HostUnreachable,
                ConnectionErrorKind::AuthFailed,
                ConnectionErrorKind::DatabaseNotFound,
                ConnectionErrorKind::Timeout,
                ConnectionErrorKind::Unknown,
            ];

            for kind in kinds {
                assert!(!kind.hint().is_empty(), "Hint for {:?} is empty", kind);
            }
        }
    }

    mod connection_error_info {
        use super::*;

        #[test]
        fn creates_from_stderr_with_auto_classification() {
            let stderr = "psql: command not found";
            let info = ConnectionErrorInfo::new(stderr);

            assert_eq!(info.kind, ConnectionErrorKind::PsqlNotFound);
            assert_eq!(info.raw_details, stderr);
        }

        #[test]
        fn creates_with_explicit_kind() {
            let stderr = "Some error";
            let info = ConnectionErrorInfo::with_kind(ConnectionErrorKind::Timeout, stderr);

            assert_eq!(info.kind, ConnectionErrorKind::Timeout);
            assert_eq!(info.raw_details, "Some error");
        }

        #[test]
        fn provides_summary_and_hint() {
            let info = ConnectionErrorInfo::new("psql: command not found");

            assert_eq!(info.summary(), "psql command not found");
            assert_eq!(info.hint(), "Install PostgreSQL or add psql to PATH");
        }
    }

    mod password_masking {
        use super::*;

        #[test]
        fn masks_password_in_postgres_url() {
            let text = "postgres://user:secretpass@localhost:5432/db";
            let masked = ConnectionErrorInfo::mask_password(text);

            assert_eq!(masked, "postgres://user:****@localhost:5432/db");
            assert!(!masked.contains("secretpass"));
        }

        #[test]
        fn masks_password_parameter() {
            let text = "connection string: password=mysecret host=localhost";
            let masked = ConnectionErrorInfo::mask_password(text);

            assert_eq!(masked, "connection string: password=**** host=localhost");
            assert!(!masked.contains("mysecret"));
        }

        #[test]
        fn masks_pgpassword_env() {
            let text = "PGPASSWORD=secret123 psql -h localhost";
            let masked = ConnectionErrorInfo::mask_password(text);

            assert_eq!(masked, "PGPASSWORD=**** psql -h localhost");
            assert!(!masked.contains("secret123"));
        }

        #[test]
        fn preserves_text_without_password() {
            let text = "psql: error: connection refused";
            let masked = ConnectionErrorInfo::mask_password(text);

            assert_eq!(masked, text);
        }

        #[test]
        fn masks_multiple_passwords() {
            let text = "postgres://u:p1@host1 and postgres://u:p2@host2";
            let masked = ConnectionErrorInfo::mask_password(text);

            assert!(!masked.contains("p1"));
            assert!(!masked.contains("p2"));
            assert!(masked.contains("****"));
        }

        #[test]
        fn info_stores_both_raw_and_masked() {
            let stderr = "Error connecting to postgres://user:secret@host";
            let info = ConnectionErrorInfo::new(stderr);

            assert!(info.raw_details.contains("secret"));
            assert!(!info.masked_details.contains("secret"));
            assert!(info.masked_details.contains("****"));
        }
    }
}
