use super::MySqlDiagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteExecutionResult {
    pub affected_rows: usize,
    pub execution_time_ms: u64,
    pub mysql_diagnostics: Vec<MySqlDiagnostic>,
}
