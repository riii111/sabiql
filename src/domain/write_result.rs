#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteExecutionResult {
    pub affected_rows: usize,
    pub diagnostics: Vec<super::DatabaseDiagnostic>,
}
