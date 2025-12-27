#[derive(Debug, Clone)]
pub struct RlsInfo {
    pub enabled: bool,
    pub force: bool,
    pub policies: Vec<RlsPolicy>,
}

#[derive(Debug, Clone)]
pub struct RlsPolicy {
    pub name: String,
    pub permissive: bool,
    pub roles: Vec<String>,
    pub cmd: RlsCommand,
    pub qual: Option<String>,
    pub with_check: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RlsCommand {
    #[default]
    All,
    Select,
    Insert,
    Update,
    Delete,
}

impl RlsInfo {
    pub fn status_display(&self) -> &'static str {
        match (self.enabled, self.force) {
            (true, true) => "ENABLED (FORCED)",
            (true, false) => "ENABLED",
            (false, _) => "DISABLED",
        }
    }
}

impl std::fmt::Display for RlsCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RlsCommand::All => write!(f, "ALL"),
            RlsCommand::Select => write!(f, "SELECT"),
            RlsCommand::Insert => write!(f, "INSERT"),
            RlsCommand::Update => write!(f, "UPDATE"),
            RlsCommand::Delete => write!(f, "DELETE"),
        }
    }
}
