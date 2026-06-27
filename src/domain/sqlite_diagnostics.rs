#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DiagnosticField {
    #[default]
    Unavailable,
    Ok(String),
    Err(String),
}

impl DiagnosticField {
    pub fn ok(value: impl Into<String>) -> Self {
        Self::Ok(value.into())
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self::Err(message.into())
    }

    pub fn ok_value(&self) -> Option<&str> {
        match self {
            Self::Ok(value) => Some(value.as_str()),
            Self::Unavailable | Self::Err(_) => None,
        }
    }

    pub fn err_message(&self) -> Option<&str> {
        match self {
            Self::Err(message) => Some(message.as_str()),
            Self::Unavailable | Self::Ok(_) => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Ok(value) => value.clone(),
            Self::Err(error) => format!("(failed: {error})"),
            Self::Unavailable => "(unavailable)".to_string(),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SqliteDiagnosticsSnapshot {
    pub db_file: DiagnosticField,
    pub sqlite_version: DiagnosticField,
    pub foreign_keys: DiagnosticField,
    pub journal_mode: DiagnosticField,
    pub query_only: DiagnosticField,
    pub busy_timeout: DiagnosticField,
    pub database_list: DiagnosticField,
    pub quick_check: DiagnosticField,
}

impl SqliteDiagnosticsSnapshot {
    pub fn quick_check_is_ok(&self) -> Option<bool> {
        self.quick_check
            .ok_value()
            .map(|summary| summary.eq_ignore_ascii_case("ok"))
    }

    #[must_use]
    pub fn core_fetch_failed(db_file: DiagnosticField) -> Self {
        Self {
            db_file,
            sqlite_version: DiagnosticField::Unavailable,
            foreign_keys: DiagnosticField::Unavailable,
            journal_mode: DiagnosticField::Unavailable,
            query_only: DiagnosticField::Unavailable,
            busy_timeout: DiagnosticField::Unavailable,
            database_list: DiagnosticField::Unavailable,
            quick_check: DiagnosticField::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_field_display_formats_error() {
        let field = DiagnosticField::err("timeout");

        assert_eq!(field.display(), "(failed: timeout)");
        assert!(!field.is_ok());
    }

    #[test]
    fn diagnostic_field_rejects_invalid_public_construction() {
        let field = DiagnosticField::ok("value");

        assert_eq!(field.ok_value(), Some("value"));
        assert!(field.err_message().is_none());
    }

    #[test]
    fn quick_check_is_ok_detects_ok_summary() {
        let snapshot = SqliteDiagnosticsSnapshot {
            quick_check: DiagnosticField::ok("ok"),
            ..Default::default()
        };

        assert_eq!(snapshot.quick_check_is_ok(), Some(true));
    }

    #[test]
    fn quick_check_is_ok_detects_failure_summary() {
        let snapshot = SqliteDiagnosticsSnapshot {
            quick_check: DiagnosticField::ok("row 1 missing from index idx_users"),
            ..Default::default()
        };

        assert_eq!(snapshot.quick_check_is_ok(), Some(false));
    }

    #[test]
    fn core_fetch_failed_marks_non_db_file_fields_unavailable() {
        let snapshot = SqliteDiagnosticsSnapshot::core_fetch_failed(DiagnosticField::err("boom"));

        assert!(snapshot.db_file.is_err());
        assert_eq!(snapshot.sqlite_version, DiagnosticField::Unavailable);
        assert_eq!(snapshot.quick_check, DiagnosticField::Unavailable);
    }
}
