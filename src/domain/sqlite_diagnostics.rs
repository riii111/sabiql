#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiagnosticField {
    pub value: Option<String>,
    pub error: Option<String>,
}

impl DiagnosticField {
    pub fn ok(value: impl Into<String>) -> Self {
        Self {
            value: Some(value.into()),
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            value: None,
            error: Some(message.into()),
        }
    }

    pub fn display(&self) -> String {
        if let Some(value) = &self.value {
            value.clone()
        } else if let Some(error) = &self.error {
            format!("(failed: {error})")
        } else {
            "(unavailable)".to_string()
        }
    }

    pub fn is_ok(&self) -> bool {
        self.value.is_some() && self.error.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickCheckResult {
    pub summary: String,
    pub is_ok: bool,
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
    pub fn quick_check_result(&self) -> Option<QuickCheckResult> {
        self.quick_check
            .value
            .as_ref()
            .map(|summary| QuickCheckResult {
                summary: summary.clone(),
                is_ok: self.quick_check.is_ok() && summary.eq_ignore_ascii_case("ok"),
            })
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
    fn quick_check_result_detects_ok_summary() {
        let snapshot = SqliteDiagnosticsSnapshot {
            quick_check: DiagnosticField::ok("ok"),
            ..Default::default()
        };

        let result = snapshot.quick_check_result().unwrap();

        assert!(result.is_ok);
        assert_eq!(result.summary, "ok");
    }
}
