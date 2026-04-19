use std::str::FromStr;

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
            Self::All => write!(f, "ALL"),
            Self::Select => write!(f, "SELECT"),
            Self::Insert => write!(f, "INSERT"),
            Self::Update => write!(f, "UPDATE"),
            Self::Delete => write!(f, "DELETE"),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ParseRlsCommandError {
    #[error("invalid RLS command: {input}")]
    Invalid { input: String },
}

impl FromStr for RlsCommand {
    type Err = ParseRlsCommandError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "*" => Ok(Self::All),
            "r" => Ok(Self::Select),
            "a" => Ok(Self::Insert),
            "w" => Ok(Self::Update),
            "d" => Ok(Self::Delete),
            input if input.eq_ignore_ascii_case("ALL") => Ok(Self::All),
            input if input.eq_ignore_ascii_case("SELECT") => Ok(Self::Select),
            input if input.eq_ignore_ascii_case("INSERT") => Ok(Self::Insert),
            input if input.eq_ignore_ascii_case("UPDATE") => Ok(Self::Update),
            input if input.eq_ignore_ascii_case("DELETE") => Ok(Self::Delete),
            _ => Err(ParseRlsCommandError::Invalid {
                input: s.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(RlsCommand::All)]
    #[case(RlsCommand::Select)]
    #[case(RlsCommand::Insert)]
    #[case(RlsCommand::Update)]
    #[case(RlsCommand::Delete)]
    fn display_round_trips(#[case] command: RlsCommand) {
        assert_eq!(command.to_string().parse::<RlsCommand>().unwrap(), command);
    }

    #[rstest]
    #[case("*", RlsCommand::All)]
    #[case("r", RlsCommand::Select)]
    #[case("a", RlsCommand::Insert)]
    #[case("w", RlsCommand::Update)]
    #[case("d", RlsCommand::Delete)]
    #[case("select", RlsCommand::Select)]
    fn from_str_accepts_codes_and_labels(#[case] input: &str, #[case] expected: RlsCommand) {
        assert_eq!(input.parse::<RlsCommand>().unwrap(), expected);
    }

    #[test]
    fn from_str_rejects_unknown_command() {
        assert!(matches!(
            "x".parse::<RlsCommand>(),
            Err(ParseRlsCommandError::Invalid { .. })
        ));
    }
}
