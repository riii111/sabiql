use super::query_result::RefreshScope;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandTag {
    Select(u64),
    Insert(u64),
    Affected(u64),
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
            || matches!(self, Self::Other(tag) if matches!(tag.as_str(), "ATTACH" | "DETACH"))
    }

    pub fn needs_refresh(&self) -> bool {
        matches!(
            self,
            Self::Insert(_)
                | Self::Affected(_)
                | Self::Update(_)
                | Self::Delete(_)
                | Self::Create(_)
                | Self::Drop(_)
                | Self::Alter(_)
                | Self::Truncate
        ) || matches!(
            self,
            Self::Other(tag)
                if matches!(
                    tag.as_str(),
                    "ANALYZE" | "ATTACH" | "DETACH" | "REINDEX" | "VACUUM"
                )
        )
    }

    pub fn refresh_scope(&self) -> RefreshScope {
        if self.is_schema_modifying() {
            RefreshScope::Metadata
        } else if self.needs_refresh() {
            RefreshScope::Data
        } else {
            RefreshScope::None
        }
    }

    pub fn affected_rows(&self) -> Option<u64> {
        match self {
            Self::Select(n)
            | Self::Insert(n)
            | Self::Affected(n)
            | Self::Update(n)
            | Self::Delete(n) => Some(*n),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affected_rows_returns_count_for_dml() {
        assert_eq!(CommandTag::Select(5).affected_rows(), Some(5));
        assert_eq!(CommandTag::Insert(3).affected_rows(), Some(3));
        assert_eq!(CommandTag::Affected(2).affected_rows(), Some(2));
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
    fn is_schema_modifying_true_for_ddl() {
        assert!(CommandTag::Create("TABLE".to_string()).is_schema_modifying());
        assert!(CommandTag::Drop("TABLE".to_string()).is_schema_modifying());
        assert!(CommandTag::Alter("TABLE".to_string()).is_schema_modifying());
    }

    #[test]
    fn is_schema_modifying_false_for_non_ddl() {
        assert!(!CommandTag::Select(0).is_schema_modifying());
        assert!(!CommandTag::Insert(1).is_schema_modifying());
        assert!(!CommandTag::Affected(1).is_schema_modifying());
        assert!(!CommandTag::Update(1).is_schema_modifying());
        assert!(!CommandTag::Delete(1).is_schema_modifying());
        assert!(!CommandTag::Truncate.is_schema_modifying());
        assert!(!CommandTag::Begin.is_schema_modifying());
        assert!(!CommandTag::Commit.is_schema_modifying());
        assert!(!CommandTag::Rollback.is_schema_modifying());
        assert!(!CommandTag::Other("VACUUM".to_string()).is_schema_modifying());
    }

    #[test]
    fn is_schema_modifying_true_for_sqlite_attachment_changes() {
        assert!(CommandTag::Other("ATTACH".to_string()).is_schema_modifying());
        assert!(CommandTag::Other("DETACH".to_string()).is_schema_modifying());
    }

    #[test]
    fn needs_refresh_true_for_dml_and_ddl() {
        assert!(CommandTag::Insert(1).needs_refresh());
        assert!(CommandTag::Affected(1).needs_refresh());
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
        assert!(!CommandTag::Other("SAVEPOINT".to_string()).needs_refresh());
    }

    #[test]
    fn needs_refresh_true_for_sqlite_side_effects() {
        for tag in ["ANALYZE", "ATTACH", "DETACH", "REINDEX", "VACUUM"] {
            assert!(CommandTag::Other(tag.to_string()).needs_refresh());
        }
    }
}
