use crate::domain::connection::DatabaseType;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::model::sql_editor::modal::SqlModalTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorInfoField {
    Owner,
    Comment,
    RowCount,
    Schema,
    TableName,
    TableKind,
    TableFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExplainCapability {
    None,
    QueryPlanOnly,
    QueryPlanAndAnalyze,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCapabilities {
    explain: ExplainCapability,
    supports_plan_comparison: bool,
    supports_er_diagram: bool,
    supports_jsonb_detail: bool,
    database_type: Option<DatabaseType>,
    supported_inspector_tabs: Vec<InspectorTab>,
    supported_inspector_info_fields: Vec<InspectorInfoField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CapabilityFlags {
    explain: ExplainCapability,
    supports_plan_comparison: bool,
    supports_er_diagram: bool,
    supports_jsonb_detail: bool,
}

impl CapabilityFlags {
    const NONE: Self = Self {
        explain: ExplainCapability::None,
        supports_plan_comparison: false,
        supports_er_diagram: false,
        supports_jsonb_detail: false,
    };

    const POSTGRESQL: Self = Self {
        explain: ExplainCapability::QueryPlanAndAnalyze,
        supports_plan_comparison: true,
        supports_er_diagram: true,
        supports_jsonb_detail: true,
    };

    const SQLITE: Self = Self {
        explain: ExplainCapability::QueryPlanOnly,
        supports_plan_comparison: false,
        supports_er_diagram: false,
        supports_jsonb_detail: false,
    };
}

impl DbCapabilities {
    fn new(
        flags: CapabilityFlags,
        database_type: Option<DatabaseType>,
        supported_inspector_tabs: Vec<InspectorTab>,
        supported_inspector_info_fields: Vec<InspectorInfoField>,
    ) -> Self {
        assert!(
            !supported_inspector_tabs.is_empty(),
            "DbCapabilities requires at least one supported inspector tab"
        );
        assert!(
            !supported_inspector_info_fields.is_empty(),
            "DbCapabilities requires at least one supported inspector info field"
        );
        assert!(
            has_unique_items(&supported_inspector_tabs),
            "DbCapabilities supported inspector tabs must be unique"
        );
        assert!(
            has_unique_items(&supported_inspector_info_fields),
            "DbCapabilities supported inspector info fields must be unique"
        );
        Self {
            explain: flags.explain,
            supports_plan_comparison: flags.supports_plan_comparison,
            supports_er_diagram: flags.supports_er_diagram,
            supports_jsonb_detail: flags.supports_jsonb_detail,
            database_type,
            supported_inspector_tabs,
            supported_inspector_info_fields,
        }
    }

    pub fn disconnected() -> Self {
        Self::new(
            CapabilityFlags::NONE,
            None,
            vec![InspectorTab::Info],
            vec![InspectorInfoField::Schema, InspectorInfoField::TableName],
        )
    }

    pub fn postgres_like() -> Self {
        Self::new(
            CapabilityFlags::POSTGRESQL,
            Some(DatabaseType::PostgreSQL),
            vec![
                InspectorTab::Info,
                InspectorTab::Columns,
                InspectorTab::Indexes,
                InspectorTab::ForeignKeys,
                InspectorTab::Rls,
                InspectorTab::Triggers,
                InspectorTab::Ddl,
            ],
            vec![
                InspectorInfoField::Owner,
                InspectorInfoField::Comment,
                InspectorInfoField::RowCount,
                InspectorInfoField::Schema,
                InspectorInfoField::TableName,
            ],
        )
    }

    pub fn sqlite_like() -> Self {
        Self::new(
            CapabilityFlags::SQLITE,
            Some(DatabaseType::SQLite),
            vec![
                InspectorTab::Info,
                InspectorTab::Columns,
                InspectorTab::Indexes,
                InspectorTab::ForeignKeys,
                InspectorTab::Triggers,
                InspectorTab::Ddl,
            ],
            vec![
                InspectorInfoField::RowCount,
                InspectorInfoField::Schema,
                InspectorInfoField::TableName,
                InspectorInfoField::TableKind,
                InspectorInfoField::TableFlags,
            ],
        )
    }

    pub fn for_database_type(database_type: DatabaseType) -> Self {
        match database_type {
            DatabaseType::PostgreSQL => Self::postgres_like(),
            DatabaseType::SQLite => Self::sqlite_like(),
        }
    }

    pub fn supports_explain(&self) -> bool {
        !matches!(self.explain, ExplainCapability::None)
    }

    pub fn supports_explain_analyze(&self) -> bool {
        matches!(self.explain, ExplainCapability::QueryPlanAndAnalyze)
    }

    pub fn supports_er_diagram(&self) -> bool {
        self.supports_er_diagram
    }

    pub fn supports_plan_comparison(&self) -> bool {
        self.supports_plan_comparison
    }

    pub fn supports_jsonb_detail(&self) -> bool {
        self.supports_jsonb_detail
    }

    pub fn supports_sqlite_diagnostics(&self) -> bool {
        self.database_type == Some(DatabaseType::SQLite)
    }

    pub fn supported_inspector_tabs(&self) -> &[InspectorTab] {
        &self.supported_inspector_tabs
    }

    pub fn supported_inspector_info_fields(&self) -> &[InspectorInfoField] {
        &self.supported_inspector_info_fields
    }

    pub fn inspector_info_line_count(&self) -> usize {
        self.supported_inspector_info_fields.len()
    }

    pub fn supports_inspector_tab(&self, tab: InspectorTab) -> bool {
        self.supported_inspector_tabs.contains(&tab)
    }

    pub fn supported_sql_modal_tabs(&self) -> &'static [SqlModalTab] {
        match (self.supports_explain(), self.supports_plan_comparison()) {
            (true, true) => &[SqlModalTab::Sql, SqlModalTab::Plan, SqlModalTab::Compare],
            (true, false) => &[SqlModalTab::Sql, SqlModalTab::Plan],
            (false, _) => &[SqlModalTab::Sql],
        }
    }

    pub fn normalize_sql_modal_tab(&self, tab: SqlModalTab) -> SqlModalTab {
        if self.supported_sql_modal_tabs().contains(&tab) {
            tab
        } else {
            SqlModalTab::Sql
        }
    }

    pub fn next_sql_modal_tab(&self, current: SqlModalTab) -> SqlModalTab {
        self.cycle_sql_modal_tab(current, 1)
    }

    pub fn prev_sql_modal_tab(&self, current: SqlModalTab) -> SqlModalTab {
        self.cycle_sql_modal_tab(current, -1)
    }

    pub fn normalize_inspector_tab(&self, tab: InspectorTab) -> InspectorTab {
        if self.supports_inspector_tab(tab) {
            tab
        } else {
            self.supported_inspector_tabs
                .first()
                .copied()
                .expect("supported_inspector_tabs must be non-empty (enforced by new())")
        }
    }

    pub fn next_inspector_tab(&self, current: InspectorTab) -> InspectorTab {
        self.cycle_inspector_tab(current, 1)
    }

    pub fn prev_inspector_tab(&self, current: InspectorTab) -> InspectorTab {
        self.cycle_inspector_tab(current, -1)
    }

    fn cycle_inspector_tab(&self, current: InspectorTab, delta: isize) -> InspectorTab {
        let tabs = &self.supported_inspector_tabs;
        let current = self.normalize_inspector_tab(current);
        let current_idx = tabs.iter().position(|tab| *tab == current).unwrap_or(0) as isize;
        let next_idx = (current_idx + delta).rem_euclid(tabs.len() as isize) as usize;
        tabs[next_idx]
    }

    fn cycle_sql_modal_tab(&self, current: SqlModalTab, delta: isize) -> SqlModalTab {
        let tabs = self.supported_sql_modal_tabs();
        let current = self.normalize_sql_modal_tab(current);
        let current_idx = tabs.iter().position(|tab| *tab == current).unwrap_or(0) as isize;
        let next_idx = (current_idx + delta).rem_euclid(tabs.len() as isize) as usize;
        tabs[next_idx]
    }
}

fn has_unique_items<T: Eq>(items: &[T]) -> bool {
    !items
        .iter()
        .enumerate()
        .any(|(idx, item)| items[idx + 1..].contains(item))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod factory {
        use super::*;

        #[test]
        fn postgresql_enables_full_inspector_surface() {
            let caps = DbCapabilities::postgres_like();

            assert!(caps.supports_explain());
            assert!(caps.supports_explain_analyze());
            assert!(caps.supports_plan_comparison());
            assert!(caps.supports_er_diagram());
            assert!(caps.supports_jsonb_detail());
            assert!(!caps.supports_sqlite_diagnostics());
            assert!(caps.supports_inspector_tab(InspectorTab::Ddl));
            assert_eq!(caps.supported_inspector_tabs().len(), 7);
            assert_eq!(
                caps.supported_inspector_info_fields(),
                &[
                    InspectorInfoField::Owner,
                    InspectorInfoField::Comment,
                    InspectorInfoField::RowCount,
                    InspectorInfoField::Schema,
                    InspectorInfoField::TableName,
                ]
            );
        }

        #[test]
        fn sqlite_omits_postgresql_only_info_fields() {
            let caps = DbCapabilities::sqlite_like();

            assert!(caps.supports_explain());
            assert!(!caps.supports_explain_analyze());
            assert!(!caps.supports_plan_comparison());
            assert!(!caps.supports_er_diagram());
            assert!(!caps.supports_jsonb_detail());
            assert!(caps.supports_sqlite_diagnostics());
            assert_eq!(
                caps.supported_inspector_tabs(),
                &[
                    InspectorTab::Info,
                    InspectorTab::Columns,
                    InspectorTab::Indexes,
                    InspectorTab::ForeignKeys,
                    InspectorTab::Triggers,
                    InspectorTab::Ddl
                ]
            );
            assert_eq!(
                caps.supported_inspector_info_fields(),
                &[
                    InspectorInfoField::RowCount,
                    InspectorInfoField::Schema,
                    InspectorInfoField::TableName,
                    InspectorInfoField::TableKind,
                    InspectorInfoField::TableFlags,
                ]
            );
            assert_eq!(
                caps.supported_sql_modal_tabs(),
                &[SqlModalTab::Sql, SqlModalTab::Plan]
            );
        }

        #[test]
        fn database_type_selects_database_specific_capabilities() {
            assert_eq!(
                DbCapabilities::for_database_type(DatabaseType::PostgreSQL),
                DbCapabilities::postgres_like()
            );
            assert_eq!(
                DbCapabilities::for_database_type(DatabaseType::SQLite),
                DbCapabilities::sqlite_like()
            );
        }

        #[test]
        fn disconnected_keeps_minimum_info_surface() {
            let caps = DbCapabilities::disconnected();

            assert!(!caps.supports_explain());
            assert!(!caps.supports_explain_analyze());
            assert!(!caps.supports_plan_comparison());
            assert!(!caps.supports_er_diagram());
            assert!(!caps.supports_jsonb_detail());
            assert_eq!(caps.supported_inspector_tabs(), &[InspectorTab::Info]);
            assert_eq!(
                caps.supported_inspector_info_fields(),
                &[InspectorInfoField::Schema, InspectorInfoField::TableName]
            );
            assert_eq!(caps.supported_sql_modal_tabs(), &[SqlModalTab::Sql]);
        }
    }

    mod normalization {
        use super::*;

        #[test]
        fn unsupported_inspector_tab_returns_first_supported_tab() {
            let caps = DbCapabilities::new(
                CapabilityFlags::NONE,
                None,
                vec![InspectorTab::Info, InspectorTab::Columns],
                vec![InspectorInfoField::Owner],
            );

            assert_eq!(
                caps.normalize_inspector_tab(InspectorTab::Triggers),
                InspectorTab::Info
            );
        }

        #[test]
        fn supported_sql_modal_tab_passes_through() {
            let caps = DbCapabilities::new(
                CapabilityFlags::POSTGRESQL,
                Some(DatabaseType::PostgreSQL),
                vec![InspectorTab::Info],
                vec![InspectorInfoField::Owner],
            );

            assert_eq!(
                caps.normalize_sql_modal_tab(SqlModalTab::Compare),
                SqlModalTab::Compare
            );
        }

        #[test]
        fn sqlite_compare_tab_normalizes_to_sql() {
            let sqlite_caps = DbCapabilities::sqlite_like();

            assert_eq!(
                sqlite_caps.normalize_sql_modal_tab(SqlModalTab::Compare),
                SqlModalTab::Sql
            );
        }

        #[test]
        fn unsupported_sql_modal_tab_returns_sql() {
            let no_explain_caps = DbCapabilities::new(
                CapabilityFlags::NONE,
                None,
                vec![InspectorTab::Info],
                vec![InspectorInfoField::Owner],
            );

            assert_eq!(
                no_explain_caps.normalize_sql_modal_tab(SqlModalTab::Plan),
                SqlModalTab::Sql
            );
        }
    }

    mod validation {
        use super::*;

        #[test]
        #[should_panic(expected = "DbCapabilities requires at least one supported inspector tab")]
        fn rejects_empty_supported_inspector_tabs() {
            let _ = DbCapabilities::new(
                CapabilityFlags::NONE,
                None,
                vec![],
                vec![InspectorInfoField::Owner],
            );
        }

        #[test]
        #[should_panic(
            expected = "DbCapabilities requires at least one supported inspector info field"
        )]
        fn rejects_empty_supported_inspector_info_fields() {
            let _ = DbCapabilities::new(
                CapabilityFlags::NONE,
                None,
                vec![InspectorTab::Info],
                vec![],
            );
        }

        #[test]
        #[should_panic(expected = "DbCapabilities supported inspector tabs must be unique")]
        fn rejects_duplicate_supported_inspector_tabs() {
            let _ = DbCapabilities::new(
                CapabilityFlags::NONE,
                None,
                vec![InspectorTab::Info, InspectorTab::Info],
                vec![InspectorInfoField::Schema],
            );
        }

        #[test]
        #[should_panic(expected = "DbCapabilities supported inspector info fields must be unique")]
        fn rejects_duplicate_supported_inspector_info_fields() {
            let _ = DbCapabilities::new(
                CapabilityFlags::NONE,
                None,
                vec![InspectorTab::Info],
                vec![InspectorInfoField::Schema, InspectorInfoField::Schema],
            );
        }
    }
}
