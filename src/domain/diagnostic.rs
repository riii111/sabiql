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
