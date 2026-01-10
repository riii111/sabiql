use std::sync::OnceLock;

use regex::Regex;

static URL_RE: OnceLock<Regex> = OnceLock::new();
static PARAM_RE: OnceLock<Regex> = OnceLock::new();
static ENV_RE: OnceLock<Regex> = OnceLock::new();

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
        let url_re = URL_RE.get_or_init(|| {
            Regex::new(r"(?i)(postgres(?:ql)?://[^:]+:)[^@]+(@)").unwrap()
        });
        let result = url_re.replace_all(text, "${1}****${2}");

        let param_re = PARAM_RE.get_or_init(|| Regex::new(r"(?i)(password=)[^\s]+").unwrap());
        let result = param_re.replace_all(&result, "${1}****");

        let env_re = ENV_RE.get_or_init(|| Regex::new(r"(PGPASSWORD=)[^\s]+").unwrap());
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
    use rstest::rstest;

    mod classify {
        use super::*;

        #[rstest]
        #[case("psql: command not found", ConnectionErrorKind::PsqlNotFound)]
        #[case("/bin/sh: psql: command not found", ConnectionErrorKind::PsqlNotFound)]
        #[case("zsh: command not found: psql", ConnectionErrorKind::PsqlNotFound)]
        #[case(r#"psql: error: could not translate host name "host" to address: nodename nor servname provided"#, ConnectionErrorKind::HostUnreachable)]
        #[case(r#"psql: error: could not translate host name "host" to address: Name or service not known"#, ConnectionErrorKind::HostUnreachable)]
        #[case(
            r#"FATAL: password authentication failed for user "user""#,
            ConnectionErrorKind::AuthFailed
        )]
        #[case(
            r#"psql: error: FATAL:  password authentication failed"#,
            ConnectionErrorKind::AuthFailed
        )]
        #[case(
            r#"FATAL: database "nonexistent" does not exist"#,
            ConnectionErrorKind::DatabaseNotFound
        )]
        #[case("psql: error: timeout expired", ConnectionErrorKind::Timeout)]
        #[case("Connection timed out", ConnectionErrorKind::Timeout)]
        #[case("Connection refused", ConnectionErrorKind::Unknown)]
        #[case("Some random error", ConnectionErrorKind::Unknown)]
        #[case("", ConnectionErrorKind::Unknown)]
        fn from_stderr(#[case] stderr: &str, #[case] expected: ConnectionErrorKind) {
            assert_eq!(ConnectionErrorKind::classify(stderr), expected);
        }
    }

    mod error_kind {
        use super::*;

        #[rstest]
        #[case(ConnectionErrorKind::PsqlNotFound)]
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
            assert_eq!(info.kind, ConnectionErrorKind::PsqlNotFound);
        }

        #[test]
        fn with_kind_uses_provided_kind() {
            let info = ConnectionErrorInfo::with_kind(ConnectionErrorKind::Timeout, "error");
            assert_eq!(info.kind, ConnectionErrorKind::Timeout);
        }

        #[test]
        fn delegates_summary_and_hint() {
            let info = ConnectionErrorInfo::new("psql: command not found");
            assert_eq!(info.summary(), "psql command not found");
            assert_eq!(info.hint(), "Install PostgreSQL or add psql to PATH");
        }
    }

    mod mask_password {
        use super::*;

        #[rstest]
        #[case("postgres://user:secret@host", "postgres://user:****@host")]
        #[case("postgresql://user:secret@host", "postgresql://user:****@host")]
        #[case("POSTGRES://user:secret@host", "POSTGRES://user:****@host")]
        #[case("PostgreSQL://user:secret@host", "PostgreSQL://user:****@host")]
        #[case("password=mysecret host=localhost", "password=**** host=localhost")]
        #[case("PASSWORD=mysecret host=localhost", "PASSWORD=**** host=localhost")]
        #[case("PGPASSWORD=secret123 psql", "PGPASSWORD=**** psql")]
        #[case("no password here", "no password here")]
        fn masks_correctly(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(ConnectionErrorInfo::mask_password(input), expected);
        }

        #[test]
        fn info_stores_both_raw_and_masked() {
            let info = ConnectionErrorInfo::new("postgres://user:secret@host");
            assert!(info.raw_details.contains("secret"));
            assert!(!info.masked_details.contains("secret"));
        }
    }
}
