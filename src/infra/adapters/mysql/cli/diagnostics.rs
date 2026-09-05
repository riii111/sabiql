use crate::domain::{DatabaseDiagnostic, DiagnosticLevel};

pub(super) fn parse_mysql_cli_diagnostics(output: &[u8]) -> Vec<DatabaseDiagnostic> {
    output
        .split(|byte| matches!(byte, b'\n' | b'\r'))
        .filter_map(parse_mysql_cli_diagnostic_line)
        .collect()
}

fn parse_mysql_cli_diagnostic_line(line: &[u8]) -> Option<DatabaseDiagnostic> {
    let line = String::from_utf8_lossy(line);
    let line = line.trim();
    let (level, rest) = if let Some(rest) = line.strip_prefix("Warning (Code ") {
        (DiagnosticLevel::Warning, rest)
    } else {
        let rest = line.strip_prefix("Note (Code ")?;
        (DiagnosticLevel::Note, rest)
    };
    let (code, message) = rest.split_once("): ")?;
    Some(DatabaseDiagnostic {
        level,
        code: code.parse().ok()?,
        message: message.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_warning_note_and_ignores_client_messages() {
        let diagnostics = parse_mysql_cli_diagnostics(
            b"mysql: [Warning] Using a password on the command line interface can be insecure.\nWarning (Code 1062): Duplicate entry '1'\r\nNote (Code 1050): Table exists\n",
        );

        assert_eq!(
            diagnostics,
            vec![
                DatabaseDiagnostic {
                    level: DiagnosticLevel::Warning,
                    code: 1062,
                    message: "Duplicate entry '1'".to_string(),
                },
                DatabaseDiagnostic {
                    level: DiagnosticLevel::Note,
                    code: 1050,
                    message: "Table exists".to_string(),
                },
            ]
        );
    }
}
