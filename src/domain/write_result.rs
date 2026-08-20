#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteDiagnosticLevel {
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteDiagnostic {
    pub level: WriteDiagnosticLevel,
    pub code: u32,
    pub message: String,
}

impl WriteDiagnostic {
    #[must_use]
    pub fn display_message(&self) -> String {
        format!(
            "{} (Code {}): {}",
            self.level.as_str(),
            self.code,
            self.message
        )
    }
}

impl WriteDiagnosticLevel {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Warning => "Warning",
            Self::Note => "Note",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteExecutionResult {
    pub affected_rows: usize,
    pub execution_time_ms: u64,
    pub diagnostics: Vec<WriteDiagnostic>,
}
