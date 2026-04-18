#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandTag {
    Select(u64),
    Insert(u64),
    Update(u64),
    Delete(u64),
    Create(String),
    Drop(String),
    Alter(String),
    Truncate,
    Begin,
    Commit,
    Rollback,
    Other(String),
}

impl CommandTag {
    pub fn is_data_modifying(&self) -> bool {
        !matches!(self, Self::Select(_) | Self::Other(_))
    }

    pub fn is_schema_modifying(&self) -> bool {
        matches!(self, Self::Create(_) | Self::Drop(_) | Self::Alter(_))
    }

    pub fn needs_refresh(&self) -> bool {
        matches!(
            self,
            Self::Insert(_)
                | Self::Update(_)
                | Self::Delete(_)
                | Self::Create(_)
                | Self::Drop(_)
                | Self::Alter(_)
                | Self::Truncate
        )
    }

    pub fn affected_rows(&self) -> Option<u64> {
        match self {
            Self::Select(n) | Self::Insert(n) | Self::Update(n) | Self::Delete(n) => Some(*n),
            _ => None,
        }
    }

    pub fn display_message(&self) -> String {
        match self {
            Self::Select(n) => row_count_label(*n, "selected"),
            Self::Insert(n) => row_count_label(*n, "inserted"),
            Self::Update(n) => row_count_label(*n, "updated"),
            Self::Delete(n) => row_count_label(*n, "deleted"),
            Self::Create(obj) => format!("{} created", obj.to_lowercase()),
            Self::Drop(obj) => format!("{} dropped", obj.to_lowercase()),
            Self::Alter(obj) => format!("{} altered", obj.to_lowercase()),
            Self::Truncate => "table truncated".to_string(),
            Self::Begin => "transaction started".to_string(),
            Self::Commit => "committed".to_string(),
            Self::Rollback => "rolled back".to_string(),
            Self::Other(tag) => tag.to_lowercase(),
        }
    }
}

fn row_count_label(n: u64, verb: &str) -> String {
    if n == 1 {
        format!("1 row {verb}")
    } else {
        format!("{n} rows {verb}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affected_rows_returns_count_for_dml() {
        assert_eq!(CommandTag::Select(5).affected_rows(), Some(5));
        assert_eq!(CommandTag::Insert(3).affected_rows(), Some(3));
        assert_eq!(CommandTag::Update(1).affected_rows(), Some(1));
        assert_eq!(CommandTag::Delete(0).affected_rows(), Some(0));
    }

    #[test]
    fn affected_rows_returns_none_for_ddl_and_tcl() {
        assert_eq!(
            CommandTag::Create("TABLE".to_string()).affected_rows(),
            None
        );
        assert_eq!(CommandTag::Drop("INDEX".to_string()).affected_rows(), None);
        assert_eq!(CommandTag::Alter("TABLE".to_string()).affected_rows(), None);
        assert_eq!(CommandTag::Truncate.affected_rows(), None);
        assert_eq!(CommandTag::Begin.affected_rows(), None);
        assert_eq!(CommandTag::Commit.affected_rows(), None);
        assert_eq!(CommandTag::Rollback.affected_rows(), None);
    }

    #[test]
    fn display_message_singular_row() {
        assert_eq!(CommandTag::Insert(1).display_message(), "1 row inserted");
        assert_eq!(CommandTag::Delete(1).display_message(), "1 row deleted");
    }

    #[test]
    fn display_message_plural_rows() {
        assert_eq!(CommandTag::Select(5).display_message(), "5 rows selected");
        assert_eq!(CommandTag::Update(10).display_message(), "10 rows updated");
    }

    #[test]
    fn display_message_zero_rows() {
        assert_eq!(CommandTag::Delete(0).display_message(), "0 rows deleted");
    }

    #[test]
    fn display_message_ddl() {
        assert_eq!(
            CommandTag::Create("TABLE".to_string()).display_message(),
            "table created"
        );
        assert_eq!(
            CommandTag::Drop("INDEX".to_string()).display_message(),
            "index dropped"
        );
        assert_eq!(
            CommandTag::Alter("TABLE".to_string()).display_message(),
            "table altered"
        );
    }

    #[test]
    fn display_message_tcl() {
        assert_eq!(CommandTag::Truncate.display_message(), "table truncated");
        assert_eq!(CommandTag::Begin.display_message(), "transaction started");
        assert_eq!(CommandTag::Commit.display_message(), "committed");
        assert_eq!(CommandTag::Rollback.display_message(), "rolled back");
    }

    #[test]
    fn display_message_other() {
        assert_eq!(
            CommandTag::Other("VACUUM".to_string()).display_message(),
            "vacuum"
        );
    }

    #[test]
    fn is_schema_modifying_true_for_ddl() {
        assert!(CommandTag::Create("TABLE".to_string()).is_schema_modifying());
        assert!(CommandTag::Drop("TABLE".to_string()).is_schema_modifying());
        assert!(CommandTag::Alter("TABLE".to_string()).is_schema_modifying());
    }

    #[test]
    fn is_schema_modifying_false_for_non_ddl() {
        assert!(!CommandTag::Select(0).is_schema_modifying());
        assert!(!CommandTag::Insert(1).is_schema_modifying());
        assert!(!CommandTag::Update(1).is_schema_modifying());
        assert!(!CommandTag::Delete(1).is_schema_modifying());
        assert!(!CommandTag::Truncate.is_schema_modifying());
        assert!(!CommandTag::Begin.is_schema_modifying());
        assert!(!CommandTag::Commit.is_schema_modifying());
        assert!(!CommandTag::Rollback.is_schema_modifying());
        assert!(!CommandTag::Other("VACUUM".to_string()).is_schema_modifying());
    }

    #[test]
    fn needs_refresh_true_for_dml_and_ddl() {
        assert!(CommandTag::Insert(1).needs_refresh());
        assert!(CommandTag::Update(1).needs_refresh());
        assert!(CommandTag::Delete(1).needs_refresh());
        assert!(CommandTag::Create("TABLE".to_string()).needs_refresh());
        assert!(CommandTag::Drop("TABLE".to_string()).needs_refresh());
        assert!(CommandTag::Alter("TABLE".to_string()).needs_refresh());
        assert!(CommandTag::Truncate.needs_refresh());
    }

    #[test]
    fn needs_refresh_false_for_read_only_and_tcl() {
        assert!(!CommandTag::Select(5).needs_refresh());
        assert!(!CommandTag::Begin.needs_refresh());
        assert!(!CommandTag::Commit.needs_refresh());
        assert!(!CommandTag::Rollback.needs_refresh());
        assert!(!CommandTag::Other("VACUUM".to_string()).needs_refresh());
    }
}
