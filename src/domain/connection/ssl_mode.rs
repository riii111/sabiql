use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// SSL mode for PostgreSQL connections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SslMode {
    Disable,
    Allow,
    #[default]
    Prefer,
    Require,
    #[serde(rename = "verify-ca")]
    VerifyCa,
    #[serde(rename = "verify-full")]
    VerifyFull,
}

impl SslMode {
    pub fn all_variants() -> &'static [SslMode] {
        &[
            SslMode::Disable,
            SslMode::Allow,
            SslMode::Prefer,
            SslMode::Require,
            SslMode::VerifyCa,
            SslMode::VerifyFull,
        ]
    }
}

impl fmt::Display for SslMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SslMode::Disable => write!(f, "disable"),
            SslMode::Allow => write!(f, "allow"),
            SslMode::Prefer => write!(f, "prefer"),
            SslMode::Require => write!(f, "require"),
            SslMode::VerifyCa => write!(f, "verify-ca"),
            SslMode::VerifyFull => write!(f, "verify-full"),
        }
    }
}

impl FromStr for SslMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "disable" => Ok(SslMode::Disable),
            "allow" => Ok(SslMode::Allow),
            "prefer" => Ok(SslMode::Prefer),
            "require" => Ok(SslMode::Require),
            "verify-ca" => Ok(SslMode::VerifyCa),
            "verify-full" => Ok(SslMode::VerifyFull),
            _ => Err(format!("Unknown SSL mode: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_prefer() {
        assert_eq!(SslMode::default(), SslMode::Prefer);
    }

    #[test]
    fn from_str_parses_all_variants() {
        assert_eq!(SslMode::from_str("disable").unwrap(), SslMode::Disable);
        assert_eq!(SslMode::from_str("allow").unwrap(), SslMode::Allow);
        assert_eq!(SslMode::from_str("prefer").unwrap(), SslMode::Prefer);
        assert_eq!(SslMode::from_str("require").unwrap(), SslMode::Require);
        assert_eq!(SslMode::from_str("verify-ca").unwrap(), SslMode::VerifyCa);
        assert_eq!(
            SslMode::from_str("verify-full").unwrap(),
            SslMode::VerifyFull
        );
    }

    #[test]
    fn from_str_is_case_insensitive() {
        assert_eq!(SslMode::from_str("PREFER").unwrap(), SslMode::Prefer);
        assert_eq!(SslMode::from_str("Verify-CA").unwrap(), SslMode::VerifyCa);
    }

    #[test]
    fn from_str_returns_error_for_unknown() {
        assert!(SslMode::from_str("unknown").is_err());
    }

    #[test]
    fn display_matches_parse() {
        for variant in SslMode::all_variants() {
            let s = variant.to_string();
            let parsed = SslMode::from_str(&s).unwrap();
            assert_eq!(*variant, parsed);
        }
    }
}
