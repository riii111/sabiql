use std::fmt;

use serde::{Deserialize, Serialize};

const MAX_LENGTH: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionNameError {
    Empty,
    TooLong { len: usize, max: usize },
}

impl fmt::Display for ConnectionNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "Connection name cannot be empty"),
            Self::TooLong { len, max } => {
                write!(f, "Connection name too long: {} chars (max {})", len, max)
            }
        }
    }
}

impl std::error::Error for ConnectionNameError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionName(String);

impl ConnectionName {
    pub fn new(name: impl Into<String>) -> Result<Self, ConnectionNameError> {
        let name = name.into();
        let trimmed = name.trim();

        if trimmed.is_empty() {
            return Err(ConnectionNameError::Empty);
        }

        let char_count = trimmed.chars().count();
        if char_count > MAX_LENGTH {
            return Err(ConnectionNameError::TooLong {
                len: char_count,
                max: MAX_LENGTH,
            });
        }

        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// For case-insensitive uniqueness comparison.
    pub fn normalized(&self) -> String {
        self.0.to_lowercase()
    }
}

impl fmt::Display for ConnectionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ConnectionName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ConnectionName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ConnectionName::new(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod new {
        use super::*;

        #[rstest]
        #[case("Production", true)]
        #[case("Local Dev", true)]
        #[case("  Production  ", true)] // trimmed
        #[case("a", true)] // 1 char minimum
        #[case("", false)] // empty
        #[case("   ", false)] // whitespace only
        fn validation(#[case] input: &str, #[case] should_succeed: bool) {
            assert_eq!(ConnectionName::new(input).is_ok(), should_succeed);
        }

        #[test]
        fn exactly_50_chars_returns_ok() {
            let name = "a".repeat(50);
            assert!(ConnectionName::new(&name).is_ok());
        }

        #[test]
        fn over_50_chars_returns_too_long_error() {
            let name = "a".repeat(51);
            let result = ConnectionName::new(&name);
            assert!(matches!(
                result,
                Err(ConnectionNameError::TooLong { len: 51, max: 50 })
            ));
        }

        #[test]
        fn trims_whitespace() {
            let name = ConnectionName::new("  Production  ").unwrap();
            assert_eq!(name.as_str(), "Production");
        }
    }

    mod normalized {
        use super::*;

        #[test]
        fn returns_lowercase() {
            let name = ConnectionName::new("Production").unwrap();
            assert_eq!(name.normalized(), "production");
        }

        #[test]
        fn mixed_case_normalized_to_lowercase() {
            let name = ConnectionName::new("My Local DB").unwrap();
            assert_eq!(name.normalized(), "my local db");
        }
    }

    mod display {
        use super::*;

        #[test]
        fn formats_as_inner_string() {
            let name = ConnectionName::new("Production").unwrap();
            assert_eq!(format!("{}", name), "Production");
        }
    }

    mod serde {
        use super::*;

        #[test]
        fn serializes_to_string() {
            let name = ConnectionName::new("Production").unwrap();
            let json = serde_json::to_string(&name).unwrap();
            assert_eq!(json, "\"Production\"");
        }

        #[test]
        fn deserializes_valid_string() {
            let json = "\"Production\"";
            let name: ConnectionName = serde_json::from_str(json).unwrap();
            assert_eq!(name.as_str(), "Production");
        }

        #[test]
        fn deserialize_empty_returns_error() {
            let json = "\"\"";
            let result: Result<ConnectionName, _> = serde_json::from_str(json);
            assert!(result.is_err());
        }
    }
}
