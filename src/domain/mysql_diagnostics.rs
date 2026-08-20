#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MySqlDiagnosticLevel {
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MySqlDiagnostic {
    pub level: MySqlDiagnosticLevel,
    pub code: u32,
    pub message: String,
}

impl MySqlDiagnosticLevel {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Warning => "Warning",
            Self::Note => "Note",
        }
    }
}
