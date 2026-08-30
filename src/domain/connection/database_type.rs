use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseType {
    #[serde(rename = "postgresql")]
    #[default]
    PostgreSQL,
    #[serde(rename = "sqlite")]
    SQLite,
    #[serde(rename = "mysql")]
    MySQL,
}

impl DatabaseType {
    pub const fn all() -> &'static [Self] {
        &[Self::PostgreSQL, Self::SQLite, Self::MySQL]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PostgreSQL => "PostgreSQL",
            Self::SQLite => "SQLite",
            Self::MySQL => "MySQL",
        }
    }
}

impl fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
