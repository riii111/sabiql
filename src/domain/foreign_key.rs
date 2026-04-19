use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub name: String,
    pub from_schema: String,
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_schema: String,
    pub to_table: String,
    pub to_columns: Vec<String>,
    pub on_delete: FkAction,
    pub on_update: FkAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FkAction {
    #[default]
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

impl ForeignKey {
    pub fn referenced_table(&self) -> String {
        format!("{}.{}", self.to_schema, self.to_table)
    }
}

impl std::fmt::Display for FkAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAction => write!(f, "NO ACTION"),
            Self::Restrict => write!(f, "RESTRICT"),
            Self::Cascade => write!(f, "CASCADE"),
            Self::SetNull => write!(f, "SET NULL"),
            Self::SetDefault => write!(f, "SET DEFAULT"),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ParseFkActionError {
    #[error("invalid foreign key action: {input}")]
    Invalid { input: String },
}

impl FromStr for FkAction {
    type Err = ParseFkActionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "a" => Ok(Self::NoAction),
            "r" => Ok(Self::Restrict),
            "c" => Ok(Self::Cascade),
            "n" => Ok(Self::SetNull),
            "d" => Ok(Self::SetDefault),
            input if input.eq_ignore_ascii_case("NO ACTION") => Ok(Self::NoAction),
            input if input.eq_ignore_ascii_case("RESTRICT") => Ok(Self::Restrict),
            input if input.eq_ignore_ascii_case("CASCADE") => Ok(Self::Cascade),
            input if input.eq_ignore_ascii_case("SET NULL") => Ok(Self::SetNull),
            input if input.eq_ignore_ascii_case("SET DEFAULT") => Ok(Self::SetDefault),
            _ => Err(ParseFkActionError::Invalid {
                input: s.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn referenced_table_returns_schema_dot_table() {
        let fk = ForeignKey {
            name: "fk_order_user".to_string(),
            from_schema: "public".to_string(),
            from_table: "orders".to_string(),
            from_columns: vec!["user_id".to_string()],
            to_schema: "public".to_string(),
            to_table: "users".to_string(),
            to_columns: vec!["id".to_string()],
            on_delete: FkAction::default(),
            on_update: FkAction::default(),
        };

        assert_eq!(fk.referenced_table(), "public.users");
    }

    #[rstest]
    #[case(FkAction::NoAction)]
    #[case(FkAction::Restrict)]
    #[case(FkAction::Cascade)]
    #[case(FkAction::SetNull)]
    #[case(FkAction::SetDefault)]
    fn display_round_trips(#[case] action: FkAction) {
        assert_eq!(action.to_string().parse::<FkAction>().unwrap(), action);
    }

    #[rstest]
    #[case("a", FkAction::NoAction)]
    #[case("r", FkAction::Restrict)]
    #[case("c", FkAction::Cascade)]
    #[case("n", FkAction::SetNull)]
    #[case("d", FkAction::SetDefault)]
    #[case("no action", FkAction::NoAction)]
    #[case("restrict", FkAction::Restrict)]
    #[case("cascade", FkAction::Cascade)]
    #[case("set null", FkAction::SetNull)]
    #[case("set default", FkAction::SetDefault)]
    fn from_str_accepts_codes_and_labels(#[case] input: &str, #[case] expected: FkAction) {
        assert_eq!(input.parse::<FkAction>().unwrap(), expected);
    }

    #[test]
    fn from_str_rejects_unknown_action() {
        assert!(matches!(
            "x".parse::<FkAction>(),
            Err(ParseFkActionError::Invalid { .. })
        ));
    }
}
