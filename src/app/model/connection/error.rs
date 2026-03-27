#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionErrorKind {
    CliNotFound,
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
            || stderr_lower.contains("not found: mysql")
            || stderr_lower.contains("not recognized")
        {
            return Self::CliNotFound;
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
            Self::CliNotFound => "Database CLI not found",
            Self::HostUnreachable => "Could not resolve host",
            Self::AuthFailed => "Authentication failed",
            Self::DatabaseNotFound => "Database does not exist",
            Self::Timeout => "Connection timed out",
            Self::Unknown => "Connection failed",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::CliNotFound => "Install the database CLI (e.g. psql) and add it to PATH",
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
        let result = Self::mask_url_passwords(text);
        let result = Self::mask_kv_passwords(&result);
        Self::mask_env_passwords(&result)
    }

    fn mask_url_passwords(text: &str) -> String {
        let lower = text.to_lowercase();
        let mut result = String::with_capacity(text.len());
        let mut i = 0;

        while i < text.len() {
            let remaining = &lower[i..];
            let scheme_len = if remaining.starts_with("postgresql://") {
                "postgresql://".len()
            } else if remaining.starts_with("postgres://") {
                "postgres://".len()
            } else if remaining.starts_with("mysql://") {
                "mysql://".len()
            } else {
                0
            };

            if scheme_len > 0 {
                let after_scheme = i + scheme_len;
                if let Some(colon_off) = text[after_scheme..].find(':') {
                    let colon = after_scheme + colon_off;
                    if let Some(at_off) = text[(colon + 1)..].find('@') {
                        let at = colon + 1 + at_off;
                        result.push_str(&text[i..=colon]);
                        result.push_str("****");
                        i = at; // '@' will be pushed next iteration
                        continue;
                    }
                }
            }

            let ch = text[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }

        result
    }

    fn mask_kv_passwords(text: &str) -> String {
        let lower = text.to_lowercase();
        Self::mask_after_prefix(text, |pos| {
            let needle = "password=";
            lower[pos..].starts_with(needle).then_some(needle.len())
        })
    }

    fn mask_env_passwords(text: &str) -> String {
        const PREFIXES: &[&str] = &["PGPASSWORD=", "MYSQL_PASSWORD=", "MYSQL_PWD="];
        Self::mask_after_prefix(text, |pos| {
            PREFIXES
                .iter()
                .find_map(|p| text[pos..].starts_with(p).then_some(p.len()))
        })
    }

    fn mask_after_prefix(text: &str, find_prefix: impl Fn(usize) -> Option<usize>) -> String {
        let mut result = String::with_capacity(text.len());
        let mut i = 0;

        while i < text.len() {
            if let Some(prefix_len) = find_prefix(i) {
                let eq_end = i + prefix_len;
                result.push_str(&text[i..eq_end]);
                result.push_str("****");
                let mut j = eq_end;
                while j < text.len() && !text.as_bytes()[j].is_ascii_whitespace() {
                    j += 1;
                }
                i = j;
            } else {
                let ch = text[i..].chars().next().unwrap();
                result.push(ch);
                i += ch.len_utf8();
            }
        }

        result
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
    use rstest::rstest;

    mod classify {
        use super::*;

        #[rstest]
        #[case("psql: command not found")]
        #[case("/bin/sh: psql: command not found")]
        #[case("zsh: command not found: psql")]
        #[case("not found: mysql")]
        fn classify_stderr_as_cli_not_found(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::CliNotFound
            );
        }

        #[rstest]
        #[case(r#"psql: error: could not translate host name "host" to address: nodename nor servname provided"#)]
        #[case(r#"psql: error: could not translate host name "host" to address: Name or service not known"#)]
        fn classify_stderr_as_host_unreachable(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::HostUnreachable
            );
        }

        #[rstest]
        #[case(r#"FATAL: password authentication failed for user "user""#)]
        #[case(r"psql: error: FATAL:  password authentication failed")]
        fn classify_stderr_as_auth_failed(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::AuthFailed
            );
        }

        #[test]
        fn classify_stderr_as_database_not_found() {
            assert_eq!(
                ConnectionErrorKind::classify(r#"FATAL: database "nonexistent" does not exist"#),
                ConnectionErrorKind::DatabaseNotFound
            );
        }

        #[rstest]
        #[case("psql: error: timeout expired")]
        #[case("Connection timed out")]
        fn classify_stderr_as_timeout(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Timeout
            );
        }

        #[rstest]
        #[case("Connection refused")]
        #[case("Some random error")]
        #[case("")]
        fn classify_stderr_as_unknown_fallback(#[case] stderr: &str) {
            assert_eq!(
                ConnectionErrorKind::classify(stderr),
                ConnectionErrorKind::Unknown
            );
        }
    }

    mod error_kind {
        use super::*;

        #[rstest]
        #[case(ConnectionErrorKind::CliNotFound)]
        #[case(ConnectionErrorKind::HostUnreachable)]
        #[case(ConnectionErrorKind::AuthFailed)]
        #[case(ConnectionErrorKind::DatabaseNotFound)]
        #[case(ConnectionErrorKind::Timeout)]
        #[case(ConnectionErrorKind::Unknown)]
        fn has_non_empty_summary_and_hint(#[case] kind: ConnectionErrorKind) {
            assert!(!kind.summary().is_empty());
            assert!(!kind.hint().is_empty());
        }
    }

    mod error_info {
        use super::*;

        #[test]
        fn new_auto_classifies() {
            let info = ConnectionErrorInfo::new("psql: command not found");
            assert_eq!(info.kind, ConnectionErrorKind::CliNotFound);
        }

        #[test]
        fn with_kind_uses_provided_kind() {
            let info = ConnectionErrorInfo::with_kind(ConnectionErrorKind::Timeout, "error");
            assert_eq!(info.kind, ConnectionErrorKind::Timeout);
        }

        #[test]
        fn delegates_summary_and_hint() {
            let info = ConnectionErrorInfo::new("psql: command not found");
            assert_eq!(info.summary(), "Database CLI not found");
            assert_eq!(
                info.hint(),
                "Install the database CLI (e.g. psql) and add it to PATH"
            );
        }
    }

    mod mask_password {
        use super::*;

        #[rstest]
        #[case("postgres://user:secret@host", "postgres://user:****@host")]
        #[case("postgresql://user:secret@host", "postgresql://user:****@host")]
        #[case("POSTGRES://user:secret@host", "POSTGRES://user:****@host")]
        #[case("PostgreSQL://user:secret@host", "PostgreSQL://user:****@host")]
        fn masks_postgres_url_scheme(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(ConnectionErrorInfo::mask_password(input), expected);
        }

        #[rstest]
        #[case("password=mysecret host=localhost", "password=**** host=localhost")]
        #[case("PASSWORD=mysecret host=localhost", "PASSWORD=**** host=localhost")]
        #[case("PGPASSWORD=secret123 psql", "PGPASSWORD=**** psql")]
        fn masks_key_value_dsn(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(ConnectionErrorInfo::mask_password(input), expected);
        }

        #[rstest]
        #[case("mysql://user:secret@host", "mysql://user:****@host")]
        #[case("MYSQL_PASSWORD=secret123 mysql", "MYSQL_PASSWORD=**** mysql")]
        #[case("MYSQL_PWD=secret123 mysql", "MYSQL_PWD=**** mysql")]
        fn masks_mysql_credentials(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(ConnectionErrorInfo::mask_password(input), expected);
        }

        #[test]
        fn passthrough_when_no_password() {
            assert_eq!(
                ConnectionErrorInfo::mask_password("no password here"),
                "no password here"
            );
        }

        #[test]
        fn info_stores_both_raw_and_masked() {
            let info = ConnectionErrorInfo::new("postgres://user:secret@host");
            assert!(info.raw_details.contains("secret"));
            assert!(!info.masked_details.contains("secret"));
        }
    }
}
