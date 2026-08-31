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
    Engine,
    RowFormat,
    TableCollation,
    CreateOptions,
}

impl InspectorInfoField {
    pub const fn omit_when_empty(self) -> bool {
        matches!(
            self,
            Self::Engine | Self::RowFormat | Self::TableCollation | Self::CreateOptions
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionFeature {
    ErDiagram,
    JsonDocumentDetail,
    JsonDocumentEdit,
    SqliteDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExplainProfile {
    Unsupported,
    QueryPlanOnly,
    QueryPlanAndAnalyze,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InspectorProfile {
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

    pub(crate) fn tabs(&self) -> &'static [InspectorTab] {
        self.tabs
    }

    pub(crate) fn info_fields(&self) -> &'static [InspectorInfoField] {
        self.info_fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineFeatureProfile {
    Disconnected,
    PostgreSQL,
    SQLite,
    MySQL,
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
const MYSQL_INSPECTOR: InspectorProfile = InspectorProfile::new(
    &[
        InspectorTab::Info,
        InspectorTab::Columns,
        InspectorTab::Indexes,
        InspectorTab::ForeignKeys,
        InspectorTab::Triggers,
        InspectorTab::Ddl,
    ],
    &[
        InspectorInfoField::Comment,
        InspectorInfoField::RowCount,
        InspectorInfoField::TableName,
        InspectorInfoField::Engine,
        InspectorInfoField::RowFormat,
        InspectorInfoField::TableCollation,
        InspectorInfoField::CreateOptions,
    ],
);

const NO_CONNECTION_FEATURES: &[ConnectionFeature] = &[];
const SERVER_FEATURES: &[ConnectionFeature] = &[
    ConnectionFeature::ErDiagram,
    ConnectionFeature::JsonDocumentDetail,
    ConnectionFeature::JsonDocumentEdit,
];
const SQLITE_FEATURES: &[ConnectionFeature] = &[ConnectionFeature::SqliteDiagnostics];

impl EngineFeatureProfile {
    pub fn disconnected() -> Self {
        Self::Disconnected
    }

    pub fn postgres_like() -> Self {
        Self::PostgreSQL
    }

    pub fn sqlite_like() -> Self {
        Self::SQLite
    }

    pub fn mysql_like() -> Self {
        Self::MySQL
    }

    pub fn for_database_type(database_type: DatabaseType) -> Self {
        match database_type {
            DatabaseType::PostgreSQL => Self::postgres_like(),
            DatabaseType::SQLite => Self::sqlite_like(),
            DatabaseType::MySQL => Self::mysql_like(),
        }
    }

    pub fn supports_explain(&self) -> bool {
        !matches!(self.explain(), ExplainProfile::Unsupported)
    }

    pub fn supports_explain_analyze(&self) -> bool {
        matches!(self.explain(), ExplainProfile::QueryPlanAndAnalyze)
    }

    pub fn supports_er_diagram(&self) -> bool {
        self.supports_connection_feature(ConnectionFeature::ErDiagram)
    }

    pub fn supports_plan_comparison(&self) -> bool {
        matches!(self.explain(), ExplainProfile::QueryPlanAndAnalyze)
    }

    pub fn supports_json_document_detail(&self) -> bool {
        self.supports_connection_feature(ConnectionFeature::JsonDocumentDetail)
    }

    pub fn supports_json_document_edit(&self) -> bool {
        self.supports_connection_feature(ConnectionFeature::JsonDocumentEdit)
    }

    pub fn supports_sqlite_diagnostics(&self) -> bool {
        self.supports_connection_feature(ConnectionFeature::SqliteDiagnostics)
    }

    pub fn supported_inspector_tabs(&self) -> &'static [InspectorTab] {
        self.inspector().tabs()
    }

    pub fn supported_inspector_info_fields(&self) -> &'static [InspectorInfoField] {
        self.inspector().info_fields()
    }

    pub fn supports_inspector_tab(&self, tab: InspectorTab) -> bool {
        self.supported_inspector_tabs().contains(&tab)
    }

    pub fn supported_sql_modal_tabs(&self) -> &'static [SqlModalTab] {
        match self.explain() {
            ExplainProfile::Unsupported => &[SqlModalTab::Sql],
            ExplainProfile::QueryPlanOnly => &[SqlModalTab::Sql, SqlModalTab::Plan],
            ExplainProfile::QueryPlanAndAnalyze => {
                &[SqlModalTab::Sql, SqlModalTab::Plan, SqlModalTab::Compare]
            }
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
            self.supported_inspector_tabs()
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

    fn inspector(&self) -> InspectorProfile {
        match self {
            Self::Disconnected => DISCONNECTED_INSPECTOR,
            Self::PostgreSQL => POSTGRESQL_INSPECTOR,
            Self::SQLite => SQLITE_INSPECTOR,
            Self::MySQL => MYSQL_INSPECTOR,
        }
    }

    fn explain(&self) -> ExplainProfile {
        match self {
            Self::Disconnected => ExplainProfile::Unsupported,
            Self::SQLite => ExplainProfile::QueryPlanOnly,
            Self::PostgreSQL | Self::MySQL => ExplainProfile::QueryPlanAndAnalyze,
        }
    }

    fn connection_features(&self) -> &'static [ConnectionFeature] {
        match self {
            Self::Disconnected => NO_CONNECTION_FEATURES,
            Self::PostgreSQL | Self::MySQL => SERVER_FEATURES,
            Self::SQLite => SQLITE_FEATURES,
        }
    }

    fn supports_connection_feature(&self, feature: ConnectionFeature) -> bool {
        self.connection_features().contains(&feature)
    }

    fn cycle_inspector_tab(&self, current: InspectorTab, delta: isize) -> InspectorTab {
        let tabs = self.inspector().tabs();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn has_unique_items<T: Eq>(items: &[T]) -> bool {
        !items
            .iter()
            .enumerate()
            .any(|(idx, item)| items[idx + 1..].contains(item))
    }

    #[test]
    fn every_database_type_has_a_valid_profile() {
        for database_type in DatabaseType::all() {
            let profile = EngineFeatureProfile::for_database_type(*database_type);

            assert!(!profile.supported_inspector_tabs().is_empty());
            assert!(!profile.supported_inspector_info_fields().is_empty());
            assert!(has_unique_items(profile.inspector().tabs()));
            assert!(has_unique_items(profile.inspector().info_fields()));
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
        assert!(profile.supports_json_document_detail());
        assert!(profile.supports_json_document_edit());
        assert!(!profile.supports_sqlite_diagnostics());
        assert!(profile.supports_inspector_tab(InspectorTab::Ddl));
        assert_eq!(
            profile.supported_inspector_tabs(),
            &[
                InspectorTab::Info,
                InspectorTab::Columns,
                InspectorTab::Indexes,
                InspectorTab::ForeignKeys,
                InspectorTab::Rls,
                InspectorTab::Triggers,
                InspectorTab::Ddl,
            ]
        );
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
        assert!(!profile.supports_json_document_detail());
        assert!(!profile.supports_json_document_edit());
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
        assert_eq!(
            EngineFeatureProfile::for_database_type(DatabaseType::MySQL),
            EngineFeatureProfile::mysql_like()
        );
    }

    #[test]
    fn mysql_profile_exposes_browse_metadata_ddl_analyze_and_compare() {
        let profile = EngineFeatureProfile::mysql_like();

        assert!(profile.supports_explain());
        assert!(profile.supports_explain_analyze());
        assert!(profile.supports_plan_comparison());
        assert!(profile.supports_er_diagram());
        assert!(profile.supports_json_document_detail());
        assert!(profile.supports_json_document_edit());
        assert!(!profile.supports_sqlite_diagnostics());
        assert_eq!(
            profile.supported_inspector_tabs(),
            &[
                InspectorTab::Info,
                InspectorTab::Columns,
                InspectorTab::Indexes,
                InspectorTab::ForeignKeys,
                InspectorTab::Triggers,
                InspectorTab::Ddl,
            ]
        );
        assert_eq!(
            profile.supported_inspector_info_fields(),
            &[
                InspectorInfoField::Comment,
                InspectorInfoField::RowCount,
                InspectorInfoField::TableName,
                InspectorInfoField::Engine,
                InspectorInfoField::RowFormat,
                InspectorInfoField::TableCollation,
                InspectorInfoField::CreateOptions,
            ]
        );
        assert_eq!(
            profile.supported_sql_modal_tabs(),
            &[SqlModalTab::Sql, SqlModalTab::Plan, SqlModalTab::Compare]
        );
    }

    #[test]
    fn disconnected_profile_keeps_minimum_surface() {
        let profile = EngineFeatureProfile::disconnected();

        assert!(!profile.supports_explain());
        assert!(!profile.supports_explain_analyze());
        assert!(!profile.supports_plan_comparison());
        assert!(!profile.supports_er_diagram());
        assert!(!profile.supports_json_document_detail());
        assert!(!profile.supports_json_document_edit());
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
    fn query_plan_and_analyze_enables_plan_comparison() {
        let profile = EngineFeatureProfile::postgres_like();

        assert!(matches!(
            profile.explain(),
            ExplainProfile::QueryPlanAndAnalyze
        ));
    }
}
