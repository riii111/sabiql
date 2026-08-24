#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseDiagnostic {
    pub level: DiagnosticLevel,
    pub code: u32,
    pub message: String,
}

impl DatabaseDiagnostic {
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

impl DiagnosticLevel {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Warning => "Warning",
            Self::Note => "Note",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_level_code_and_message_for_statuses() {
        assert_eq!(
            DatabaseDiagnostic {
                level: DiagnosticLevel::Warning,
                code: 1265,
                message: "Data truncated".to_string(),
            }
            .display_message(),
            "Warning (Code 1265): Data truncated"
        );
    }
}
