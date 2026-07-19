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
pub enum ConnectionFeature {
    ErDiagram,
    JsonbDetail,
    SqliteDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonSupport {
    Unsupported,
    Supported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplainProfile {
    Unsupported,
    QueryPlanOnly,
    QueryPlanAndAnalyze { comparison: ComparisonSupport },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectorProfile {
    tabs: &'static [InspectorTab],
    info_fields: &'static [InspectorInfoField],
}

impl InspectorProfile {
    const fn new(
        tabs: &'static [InspectorTab],
        info_fields: &'static [InspectorInfoField],
    ) -> Self {
        Self { tabs, info_fields }
    }

    pub fn tabs(&self) -> &'static [InspectorTab] {
        self.tabs
    }

    pub fn info_fields(&self) -> &'static [InspectorInfoField] {
        self.info_fields
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineFeatureProfile {
    inspector: InspectorProfile,
    explain: ExplainProfile,
    connection_features: &'static [ConnectionFeature],
}

const DISCONNECTED_INSPECTOR: InspectorProfile = InspectorProfile::new(
    &[InspectorTab::Info],
    &[InspectorInfoField::Schema, InspectorInfoField::TableName],
);
const POSTGRESQL_INSPECTOR: InspectorProfile = InspectorProfile::new(
    &[
        InspectorTab::Info,
        InspectorTab::Columns,
        InspectorTab::Indexes,
        InspectorTab::ForeignKeys,
        InspectorTab::Rls,
        InspectorTab::Triggers,
        InspectorTab::Ddl,
    ],
    &[
        InspectorInfoField::Owner,
        InspectorInfoField::Comment,
        InspectorInfoField::RowCount,
        InspectorInfoField::Schema,
        InspectorInfoField::TableName,
    ],
);
const SQLITE_INSPECTOR: InspectorProfile = InspectorProfile::new(
    &[
        InspectorTab::Info,
        InspectorTab::Columns,
        InspectorTab::Indexes,
        InspectorTab::ForeignKeys,
        InspectorTab::Triggers,
        InspectorTab::Ddl,
    ],
    &[
        InspectorInfoField::RowCount,
        InspectorInfoField::Schema,
        InspectorInfoField::TableName,
        InspectorInfoField::TableKind,
        InspectorInfoField::TableFlags,
    ],
);

const NO_CONNECTION_FEATURES: &[ConnectionFeature] = &[];
const POSTGRESQL_FEATURES: &[ConnectionFeature] =
    &[ConnectionFeature::ErDiagram, ConnectionFeature::JsonbDetail];
const SQLITE_FEATURES: &[ConnectionFeature] = &[ConnectionFeature::SqliteDiagnostics];

impl EngineFeatureProfile {
    fn new(
        inspector: InspectorProfile,
        explain: ExplainProfile,
        connection_features: &'static [ConnectionFeature],
    ) -> Self {
        assert!(
            !inspector.tabs().is_empty(),
            "EngineFeatureProfile requires at least one supported inspector tab"
        );
        assert!(
            !inspector.info_fields().is_empty(),
            "EngineFeatureProfile requires at least one supported inspector info field"
        );
        assert!(
            has_unique_items(inspector.tabs()),
            "EngineFeatureProfile supported inspector tabs must be unique"
        );
        assert!(
            has_unique_items(inspector.info_fields()),
            "EngineFeatureProfile supported inspector info fields must be unique"
        );
        assert!(
            has_unique_items(connection_features),
            "EngineFeatureProfile connection features must be unique"
        );
        Self {
            inspector,
            explain,
            connection_features,
        }
    }

    pub fn disconnected() -> Self {
        Self::new(
            DISCONNECTED_INSPECTOR,
            ExplainProfile::Unsupported,
            NO_CONNECTION_FEATURES,
        )
    }

    pub fn postgres_like() -> Self {
        Self::new(
            POSTGRESQL_INSPECTOR,
            ExplainProfile::QueryPlanAndAnalyze {
                comparison: ComparisonSupport::Supported,
            },
            POSTGRESQL_FEATURES,
        )
    }

    pub fn sqlite_like() -> Self {
        Self::new(
            SQLITE_INSPECTOR,
            ExplainProfile::QueryPlanOnly,
            SQLITE_FEATURES,
        )
    }

    pub fn for_database_type(database_type: DatabaseType) -> Self {
        match database_type {
            DatabaseType::PostgreSQL => Self::postgres_like(),
            DatabaseType::SQLite => Self::sqlite_like(),
        }
    }

    pub fn inspector(&self) -> InspectorProfile {
        self.inspector
    }

    pub fn explain(&self) -> ExplainProfile {
        self.explain
    }

    pub fn connection_features(&self) -> &'static [ConnectionFeature] {
        self.connection_features
    }

    pub fn supports_explain(&self) -> bool {
        !matches!(self.explain, ExplainProfile::Unsupported)
    }

    pub fn supports_explain_analyze(&self) -> bool {
        matches!(self.explain, ExplainProfile::QueryPlanAndAnalyze { .. })
    }

    pub fn supports_er_diagram(&self) -> bool {
        self.supports_connection_feature(ConnectionFeature::ErDiagram)
    }

    pub fn supports_plan_comparison(&self) -> bool {
        matches!(
            self.explain,
            ExplainProfile::QueryPlanAndAnalyze {
                comparison: ComparisonSupport::Supported,
            }
        )
    }

    pub fn supports_jsonb_detail(&self) -> bool {
        self.supports_connection_feature(ConnectionFeature::JsonbDetail)
    }

    pub fn supports_sqlite_diagnostics(&self) -> bool {
        self.supports_connection_feature(ConnectionFeature::SqliteDiagnostics)
    }

    pub fn supported_inspector_tabs(&self) -> &'static [InspectorTab] {
        self.inspector.tabs()
    }

    pub fn supported_inspector_info_fields(&self) -> &'static [InspectorInfoField] {
        self.inspector.info_fields()
    }

    pub fn inspector_info_line_count(&self) -> usize {
        self.inspector.info_fields().len()
    }

    pub fn supports_inspector_tab(&self, tab: InspectorTab) -> bool {
        self.inspector.tabs().contains(&tab)
    }

    pub fn supported_sql_modal_tabs(&self) -> &'static [SqlModalTab] {
        match self.explain {
            ExplainProfile::Unsupported => &[SqlModalTab::Sql],
            ExplainProfile::QueryPlanOnly
            | ExplainProfile::QueryPlanAndAnalyze {
                comparison: ComparisonSupport::Unsupported,
            } => &[SqlModalTab::Sql, SqlModalTab::Plan],
            ExplainProfile::QueryPlanAndAnalyze {
                comparison: ComparisonSupport::Supported,
            } => &[SqlModalTab::Sql, SqlModalTab::Plan, SqlModalTab::Compare],
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
            self.inspector
                .tabs()
                .first()
                .copied()
                .expect("EngineFeatureProfile requires at least one supported inspector tab")
        }
    }

    pub fn next_inspector_tab(&self, current: InspectorTab) -> InspectorTab {
        self.cycle_inspector_tab(current, 1)
    }

    pub fn prev_inspector_tab(&self, current: InspectorTab) -> InspectorTab {
        self.cycle_inspector_tab(current, -1)
    }

    fn supports_connection_feature(&self, feature: ConnectionFeature) -> bool {
        self.connection_features.contains(&feature)
    }

    fn cycle_inspector_tab(&self, current: InspectorTab, delta: isize) -> InspectorTab {
        let tabs = self.inspector.tabs();
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

    #[test]
    fn every_database_type_has_a_valid_profile() {
        for database_type in DatabaseType::all() {
            let profile = EngineFeatureProfile::for_database_type(*database_type);

            assert!(!profile.supported_inspector_tabs().is_empty());
            assert!(!profile.supported_inspector_info_fields().is_empty());
            assert!(has_unique_items(profile.supported_inspector_tabs()));
            assert!(has_unique_items(profile.supported_inspector_info_fields()));
            assert!(has_unique_items(profile.connection_features()));

            if profile.supports_plan_comparison() {
                assert!(profile.supports_explain());
            }
            if profile.supports_explain_analyze() {
                assert!(profile.supports_explain());
            }
        }
    }

    #[test]
    fn postgresql_profile_enables_full_inspector_surface() {
        let profile = EngineFeatureProfile::postgres_like();

        assert!(profile.supports_explain());
        assert!(profile.supports_explain_analyze());
        assert!(profile.supports_plan_comparison());
        assert!(profile.supports_er_diagram());
        assert!(profile.supports_jsonb_detail());
        assert!(!profile.supports_sqlite_diagnostics());
        assert!(profile.supports_inspector_tab(InspectorTab::Ddl));
        assert_eq!(profile.supported_inspector_tabs().len(), 7);
        assert_eq!(
            profile.supported_inspector_info_fields(),
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
    fn sqlite_profile_omits_postgresql_only_features() {
        let profile = EngineFeatureProfile::sqlite_like();

        assert!(profile.supports_explain());
        assert!(!profile.supports_explain_analyze());
        assert!(!profile.supports_plan_comparison());
        assert!(!profile.supports_er_diagram());
        assert!(!profile.supports_jsonb_detail());
        assert!(profile.supports_sqlite_diagnostics());
        assert_eq!(
            profile.supported_inspector_tabs(),
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
            profile.supported_inspector_info_fields(),
            &[
                InspectorInfoField::RowCount,
                InspectorInfoField::Schema,
                InspectorInfoField::TableName,
                InspectorInfoField::TableKind,
                InspectorInfoField::TableFlags,
            ]
        );
        assert_eq!(
            profile.supported_sql_modal_tabs(),
            &[SqlModalTab::Sql, SqlModalTab::Plan]
        );
    }

    #[test]
    fn database_type_selects_the_matching_profile() {
        assert_eq!(
            EngineFeatureProfile::for_database_type(DatabaseType::PostgreSQL),
            EngineFeatureProfile::postgres_like()
        );
        assert_eq!(
            EngineFeatureProfile::for_database_type(DatabaseType::SQLite),
            EngineFeatureProfile::sqlite_like()
        );
    }

    #[test]
    fn disconnected_profile_keeps_minimum_surface() {
        let profile = EngineFeatureProfile::disconnected();

        assert!(!profile.supports_explain());
        assert!(!profile.supports_explain_analyze());
        assert!(!profile.supports_plan_comparison());
        assert!(!profile.supports_er_diagram());
        assert!(!profile.supports_jsonb_detail());
        assert!(!profile.supports_sqlite_diagnostics());
        assert_eq!(profile.supported_inspector_tabs(), &[InspectorTab::Info]);
        assert_eq!(
            profile.supported_inspector_info_fields(),
            &[InspectorInfoField::Schema, InspectorInfoField::TableName]
        );
        assert_eq!(profile.supported_sql_modal_tabs(), &[SqlModalTab::Sql]);
    }

    #[test]
    fn unsupported_inspector_tab_normalizes_to_first_supported_tab() {
        let profile = EngineFeatureProfile::sqlite_like();

        assert_eq!(
            profile.normalize_inspector_tab(InspectorTab::Rls),
            InspectorTab::Info
        );
    }

    #[test]
    fn sqlite_compare_tab_normalizes_to_sql() {
        let profile = EngineFeatureProfile::sqlite_like();

        assert_eq!(
            profile.normalize_sql_modal_tab(SqlModalTab::Compare),
            SqlModalTab::Sql
        );
    }

    #[test]
    fn compare_support_is_nested_under_explain_support() {
        let profile = EngineFeatureProfile::postgres_like();

        assert!(matches!(
            profile.explain(),
            ExplainProfile::QueryPlanAndAnalyze {
                comparison: ComparisonSupport::Supported,
            }
        ));
    }
}
