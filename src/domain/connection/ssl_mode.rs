use serde::{Deserialize, Serialize};
use std::fmt;

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
    pub fn all_variants() -> &'static [Self] {
        &[
            Self::Disable,
            Self::Allow,
            Self::Prefer,
            Self::Require,
            Self::VerifyCa,
            Self::VerifyFull,
        ]
    }
}

impl fmt::Display for SslMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disable => write!(f, "disable"),
            Self::Allow => write!(f, "allow"),
            Self::Prefer => write!(f, "prefer"),
            Self::Require => write!(f, "require"),
            Self::VerifyCa => write!(f, "verify-ca"),
            Self::VerifyFull => write!(f, "verify-full"),
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
    fn display_matches_wire_values() {
        let values = SslMode::all_variants()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");

        assert_eq!(values, "disable,allow,prefer,require,verify-ca,verify-full");
    }
}
