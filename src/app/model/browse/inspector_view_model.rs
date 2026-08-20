use crate::domain::{
    DatabaseType, ForeignKey, Index, IndexType, RlsInfo, Table, TriggerCreationContext,
};
use crate::model::browse::session::TableDetailState;
use crate::model::shared::engine_feature_profile::{EngineFeatureProfile, InspectorInfoField};
use crate::model::shared::inspector_tab::InspectorTab;
use crate::policy::table_kind::{inspector_flags_label, inspector_kind_label};
use crate::ports::outbound::DdlGenerator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorViewModel {
    active_tab: InspectorTab,
    load_state: InspectorLoadState,
    section: Option<InspectorSection>,
    empty_state: Option<InspectorEmptyState>,
    unavailable_reason: Option<InspectorUnavailableReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorLoadState {
    NoTableSelected,
    Loading,
    Success,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorSection {
    Info {
        rows: Vec<InspectorInfoRow>,
    },
    Columns {
        rows: Vec<InspectorColumnRow>,
        show_read_only: bool,
    },
    Indexes {
        rows: Vec<InspectorIndexRow>,
        show_type: bool,
        show_partial: bool,
        show_details: bool,
    },
    ForeignKeys {
        rows: Vec<InspectorForeignKeyRow>,
    },
    Rls {
        rows: Vec<InspectorRlsRow>,
    },
    Triggers {
        rows: Vec<InspectorTriggerRow>,
    },
    Ddl {
        rows: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorInfoRow {
    Field {
        field: InspectorInfoField,
        value: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorColumnRow {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub read_only_reason: Option<String>,
    pub default: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorIndexRow {
    pub name: String,
    pub columns: String,
    pub index_type: Option<String>,
    pub unique: bool,
    pub partial: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorForeignKeyRow {
    pub name: String,
    pub columns: String,
    pub references: String,
    pub on_update: String,
    pub on_delete: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorTriggerRow {
    pub name: String,
    pub timing: String,
    pub events: String,
    pub action_order: Option<i32>,
    pub definition: String,
    pub security_context: Option<String>,
    pub creation_context: Option<TriggerCreationContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorRlsRow {
    RlsStatus {
        enabled: bool,
        force: bool,
    },
    RlsSpacer,
    RlsPoliciesHeading,
    RlsPolicy {
        name: String,
        command: String,
        permissive: bool,
    },
    RlsPolicyQual(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorEmptyState {
    NoTableSelected,
    NoColumns,
    NoIndexes,
    NoForeignKeys,
    NoTriggers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorUnavailableReason {
    RlsNotEnabled,
}

impl InspectorViewModel {
    pub fn build_with_detail_state(
        profile: &EngineFeatureProfile,
        selected_tab: InspectorTab,
        table_detail_state: &TableDetailState,
        database_type: DatabaseType,
        ddl_generator: &dyn DdlGenerator,
    ) -> Self {
        let active_tab = profile.normalize_inspector_tab(selected_tab);
        let (load_state, table) = match table_detail_state {
            TableDetailState::NotSelected => (InspectorLoadState::NoTableSelected, None),
            TableDetailState::Loading => (InspectorLoadState::Loading, None),
            TableDetailState::Loaded(table) => (InspectorLoadState::Success, Some(table.as_ref())),
            TableDetailState::Error(error) => (InspectorLoadState::Error(error.clone()), None),
        };
        let Some(table) = table else {
            let empty_state = matches!(&load_state, InspectorLoadState::NoTableSelected)
                .then_some(InspectorEmptyState::NoTableSelected);
            return Self {
                active_tab,
                load_state,
                section: None,
                empty_state,
                unavailable_reason: None,
            };
        };

        let (section, empty_state, unavailable_reason) = match active_tab {
            InspectorTab::Info => (
                InspectorSection::Info {
                    rows: profile
                        .supported_inspector_info_fields()
                        .iter()
                        .copied()
                        .map(|field| InspectorInfoRow::Field {
                            field,
                            value: info_value(field, table),
                        })
                        .collect(),
                },
                None,
                None,
            ),
            InspectorTab::Columns => {
                let show_read_only = table
                    .columns
                    .iter()
                    .any(|column| column.read_only_reason().is_some());
                let rows = table
                    .columns
                    .iter()
                    .map(|column| InspectorColumnRow {
                        name: column.name.clone(),
                        data_type: column.data_type.clone(),
                        nullable: column.is_nullable(),
                        primary_key: column.is_primary_key(),
                        read_only_reason: column.read_only_reason().map(ToString::to_string),
                        default: column.default.clone(),
                        comment: column.comment.clone(),
                    })
                    .collect();
                (
                    InspectorSection::Columns {
                        rows,
                        show_read_only,
                    },
                    table
                        .columns
                        .is_empty()
                        .then_some(InspectorEmptyState::NoColumns),
                    None,
                )
            }
            InspectorTab::Indexes => {
                let show_type = table
                    .indexes
                    .iter()
                    .any(|index| index.index_type != IndexType::Unknown);
                let show_partial = matches!(
                    database_type,
                    DatabaseType::PostgreSQL | DatabaseType::SQLite
                );
                let show_details = table.indexes.iter().any(Index::has_index_detail);
                let rows = table
                    .indexes
                    .iter()
                    .map(|index| InspectorIndexRow {
                        name: index.name.clone(),
                        columns: index.columns.join(", "),
                        index_type: index_type_label(index),
                        unique: index.is_unique(),
                        partial: index.is_partial(),
                        detail: index.has_index_detail().then(|| index_detail(index)),
                    })
                    .collect();
                (
                    InspectorSection::Indexes {
                        rows,
                        show_type,
                        show_partial,
                        show_details,
                    },
                    table
                        .indexes
                        .is_empty()
                        .then_some(InspectorEmptyState::NoIndexes),
                    None,
                )
            }
            InspectorTab::ForeignKeys => {
                let rows = table.foreign_keys.iter().map(foreign_key_row).collect();
                (
                    InspectorSection::ForeignKeys { rows },
                    table
                        .foreign_keys
                        .is_empty()
                        .then_some(InspectorEmptyState::NoForeignKeys),
                    None,
                )
            }
            InspectorTab::Rls => match &table.rls {
                None => (
                    InspectorSection::Rls { rows: Vec::new() },
                    None,
                    Some(InspectorUnavailableReason::RlsNotEnabled),
                ),
                Some(rls) => (
                    InspectorSection::Rls {
                        rows: rls_rows(rls),
                    },
                    None,
                    None,
                ),
            },
            InspectorTab::Triggers => {
                let rows = table
                    .triggers
                    .iter()
                    .map(|trigger| InspectorTriggerRow {
                        name: trigger.name.clone(),
                        timing: trigger.timing.to_string(),
                        events: trigger
                            .events
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("/"),
                        action_order: trigger.action_order,
                        definition: trigger.definition.clone(),
                        security_context: trigger.security_context.clone(),
                        creation_context: trigger.creation_context.clone(),
                    })
                    .collect();
                (
                    InspectorSection::Triggers { rows },
                    table
                        .triggers
                        .is_empty()
                        .then_some(InspectorEmptyState::NoTriggers),
                    None,
                )
            }
            InspectorTab::Ddl => (
                InspectorSection::Ddl {
                    rows: ddl_generator
                        .generate_ddl(database_type, table)
                        .lines()
                        .map(str::to_string)
                        .collect(),
                },
                None,
                None,
            ),
        };

        Self {
            active_tab,
            load_state,
            section: Some(section),
            empty_state,
            unavailable_reason,
        }
    }

    pub fn active_tab(&self) -> InspectorTab {
        self.active_tab
    }

    pub fn load_state(&self) -> &InspectorLoadState {
        &self.load_state
    }

    pub fn section(&self) -> Option<&InspectorSection> {
        self.section.as_ref()
    }

    pub fn empty_state(&self) -> Option<InspectorEmptyState> {
        self.empty_state
    }

    pub fn unavailable_reason(&self) -> Option<InspectorUnavailableReason> {
        self.unavailable_reason
    }

    pub fn row_count(&self) -> usize {
        self.section.as_ref().map_or(0, InspectorSection::row_count)
    }

    pub fn visible_rows(&self, pane_height: u16) -> usize {
        match self.section.as_ref() {
            Some(
                InspectorSection::Info { .. }
                | InspectorSection::Rls { .. }
                | InspectorSection::Ddl { .. },
            ) => pane_height.saturating_sub(3) as usize,
            _ => pane_height.saturating_sub(5) as usize,
        }
    }

    pub fn max_scroll(&self, pane_height: u16) -> usize {
        self.row_count()
            .saturating_sub(self.visible_rows(pane_height))
    }
}

impl InspectorSection {
    pub fn row_count(&self) -> usize {
        match self {
            Self::Info { rows } => rows.len(),
            Self::Columns { rows, .. } => rows.len(),
            Self::Indexes { rows, .. } => rows.len(),
            Self::ForeignKeys { rows } => rows.len(),
            Self::Rls { rows } => rows.len(),
            Self::Triggers { rows } => rows.len(),
            Self::Ddl { rows } => rows.len(),
        }
    }
}

impl InspectorEmptyState {
    pub fn message(self) -> &'static str {
        match self {
            Self::NoTableSelected => "(select a table)",
            Self::NoColumns => "No columns",
            Self::NoIndexes => "No indexes",
            Self::NoForeignKeys => "No foreign keys",
            Self::NoTriggers => "No triggers",
        }
    }
}

impl InspectorUnavailableReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::RlsNotEnabled => "RLS not enabled",
        }
    }
}

fn info_value(field: InspectorInfoField, table: &Table) -> Option<String> {
    match field {
        InspectorInfoField::Owner => table.owner.clone(),
        InspectorInfoField::Comment => table.comment.clone(),
        InspectorInfoField::RowCount => table.row_count_estimate.map(|count| format!("~{count}")),
        InspectorInfoField::Schema => Some(table.schema.clone()),
        InspectorInfoField::TableName => Some(table.name.clone()),
        InspectorInfoField::TableKind => Some(inspector_kind_label(&table.kind_info)),
        InspectorInfoField::TableFlags => inspector_flags_label(&table.kind_info),
    }
}

fn index_detail(index: &Index) -> String {
    if index.needs_source_definition_detail()
        && let Some(definition) = &index.definition
    {
        let mut detail = definition.clone();
        if index.is_invisible() {
            detail.push_str("; invisible");
        }
        return detail;
    }

    let mut details = Vec::new();
    if index.has_expression() {
        details.push("expression");
    }
    if index.has_auxiliary_columns() {
        details.push("auxiliary-columns");
    }
    if index.has_descending_key() {
        details.push("descending");
    }
    if index.has_non_binary_collation() {
        details.push("collation");
    }
    if index.is_invisible() {
        details.push("invisible");
    }
    details.join("; ")
}

fn index_type_label(index: &Index) -> Option<String> {
    match index.index_type {
        IndexType::Unknown => None,
        _ => Some(index.index_type.to_string()),
    }
}

fn foreign_key_row(fk: &ForeignKey) -> InspectorForeignKeyRow {
    let references = format!(
        "{}.{}({})",
        fk.to_schema,
        fk.to_table,
        fk.to_columns.join(", ")
    );
    InspectorForeignKeyRow {
        name: fk.name.clone(),
        columns: fk.from_columns.join(", "),
        references: if fk.is_reference_resolved() {
            references
        } else {
            format!("{references} (unresolved)")
        },
        on_update: fk.on_update.to_string(),
        on_delete: fk.on_delete.to_string(),
    }
}

fn rls_rows(rls: &RlsInfo) -> Vec<InspectorRlsRow> {
    let mut rows = vec![InspectorRlsRow::RlsStatus {
        enabled: rls.enabled,
        force: rls.force,
    }];
    if !rls.policies.is_empty() {
        rows.push(InspectorRlsRow::RlsSpacer);
        rows.push(InspectorRlsRow::RlsPoliciesHeading);
        for policy in &rls.policies {
            rows.push(InspectorRlsRow::RlsPolicy {
                name: policy.name.clone(),
                command: policy.cmd.to_string(),
                permissive: policy.permissive,
            });
            if let Some(qual) = &policy.qual {
                rows.push(InspectorRlsRow::RlsPolicyQual(qual.clone()));
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Column, ColumnAttributes, FkAction, IndexAttributes, RlsCommand, RlsPolicy, TableKindInfo,
        Trigger, TriggerEvent, TriggerTiming,
    };

    struct TestDdlGenerator;

    impl DdlGenerator for TestDdlGenerator {
        fn generate_ddl(&self, _database_type: DatabaseType, _table: &Table) -> String {
            "CREATE TABLE users (\n  id integer\n);".to_string()
        }
    }

    fn table() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            owner: Some("owner".to_string()),
            columns: vec![Column {
                attributes: ColumnAttributes::empty(),
                name: "id".to_string(),
                data_type: "integer".to_string(),
                default: None,
                comment: None,
                ordinal_position: 1,
            }],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![ForeignKey {
                name: "users_org_id_fkey".to_string(),
                from_schema: "public".to_string(),
                from_table: "users".to_string(),
                from_columns: vec!["org_id".to_string()],
                to_schema: "public".to_string(),
                to_table: "orgs".to_string(),
                to_columns: vec!["id".to_string()],
                on_delete: FkAction::Cascade,
                on_update: FkAction::SetNull,
                reference_resolved: true,
            }],
            indexes: vec![Index {
                name: "users_pkey".to_string(),
                columns: vec!["id".to_string()],
                attributes: IndexAttributes::UNIQUE,
                index_type: IndexType::BTree,
                definition: None,
            }],
            rls: Some(RlsInfo {
                enabled: true,
                force: false,
                policies: vec![RlsPolicy {
                    name: "users_select".to_string(),
                    permissive: true,
                    roles: vec!["public".to_string()],
                    cmd: RlsCommand::Select,
                    qual: Some("true".to_string()),
                    with_check: None,
                }],
            }),
            triggers: vec![Trigger {
                name: "users_updated".to_string(),
                timing: TriggerTiming::Before,
                events: vec![TriggerEvent::Update],
                action_order: None,
                definition: "set_updated_at".to_string(),
                security_context: Some("INVOKER".to_string()),
                creation_context: None,
            }],
            row_count_estimate: Some(3),
            comment: Some("Users".to_string()),
            source_ddl: None,
            kind_info: TableKindInfo::default(),
        }
    }

    fn build_loaded(
        profile: &EngineFeatureProfile,
        selected_tab: InspectorTab,
        table: &Table,
        database_type: DatabaseType,
        ddl_generator: &dyn DdlGenerator,
    ) -> InspectorViewModel {
        InspectorViewModel::build_with_detail_state(
            profile,
            selected_tab,
            &TableDetailState::Loaded(Box::new(table.clone())),
            database_type,
            ddl_generator,
        )
    }

    #[test]
    fn no_table_exposes_empty_state_without_display_rows() {
        let model = InspectorViewModel::build_with_detail_state(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Info,
            &TableDetailState::NotSelected,
            DatabaseType::PostgreSQL,
            &TestDdlGenerator,
        );

        assert_eq!(model.row_count(), 0);
        assert_eq!(
            model.empty_state(),
            Some(InspectorEmptyState::NoTableSelected)
        );
        assert_eq!(model.unavailable_reason(), None);
    }

    #[test]
    fn loading_detail_exposes_loading_state_without_stale_rows() {
        let model = InspectorViewModel::build_with_detail_state(
            &EngineFeatureProfile::mysql_like(),
            InspectorTab::Info,
            &TableDetailState::Loading,
            DatabaseType::MySQL,
            &TestDdlGenerator,
        );

        assert_eq!(model.load_state(), &InspectorLoadState::Loading);
        assert_eq!(model.section(), None);
        assert_eq!(model.empty_state(), None);
    }

    #[test]
    fn failed_detail_exposes_error_state_without_stale_rows() {
        let model = InspectorViewModel::build_with_detail_state(
            &EngineFeatureProfile::mysql_like(),
            InspectorTab::Info,
            &TableDetailState::Error("permission denied".to_string()),
            DatabaseType::MySQL,
            &TestDdlGenerator,
        );

        assert_eq!(
            model.load_state(),
            &InspectorLoadState::Error("permission denied".to_string())
        );
        assert_eq!(model.section(), None);
    }

    #[test]
    fn mysql_info_has_only_comment_rows_and_table_name() {
        let model = build_loaded(
            &EngineFeatureProfile::mysql_like(),
            InspectorTab::Info,
            &table(),
            DatabaseType::MySQL,
            &TestDdlGenerator,
        );

        assert_eq!(model.row_count(), 3);
        match model.section() {
            Some(InspectorSection::Info { rows }) => assert_eq!(
                rows,
                &[
                    InspectorInfoRow::Field {
                        field: InspectorInfoField::Comment,
                        value: Some("Users".to_string()),
                    },
                    InspectorInfoRow::Field {
                        field: InspectorInfoField::RowCount,
                        value: Some("~3".to_string()),
                    },
                    InspectorInfoRow::Field {
                        field: InspectorInfoField::TableName,
                        value: Some("users".to_string()),
                    },
                ]
            ),
            section => panic!("expected info section, got {section:?}"),
        }
    }

    #[test]
    fn foreign_key_rows_include_referential_actions() {
        let model = build_loaded(
            &EngineFeatureProfile::mysql_like(),
            InspectorTab::ForeignKeys,
            &table(),
            DatabaseType::MySQL,
            &TestDdlGenerator,
        );

        match model.section() {
            Some(InspectorSection::ForeignKeys { rows }) => assert_eq!(
                rows,
                &[InspectorForeignKeyRow {
                    name: "users_org_id_fkey".to_string(),
                    columns: "org_id".to_string(),
                    references: "public.orgs(id)".to_string(),
                    on_update: "SET NULL".to_string(),
                    on_delete: "CASCADE".to_string(),
                }]
            ),
            section => panic!("expected foreign keys section, got {section:?}"),
        }
    }

    #[test]
    fn mysql_info_omits_schema_and_indexes_hide_partial_column() {
        let mut table = table();
        table.indexes = vec![Index {
            name: "users_email_lower".to_string(),
            columns: vec!["lower(email)".to_string()],
            attributes: IndexAttributes::EXPRESSION,
            index_type: IndexType::BTree,
            definition: Some(
                "CREATE INDEX users_email_lower ON users ((lower(email)))".to_string(),
            ),
        }];

        let info = build_loaded(
            &EngineFeatureProfile::mysql_like(),
            InspectorTab::Info,
            &table,
            DatabaseType::MySQL,
            &TestDdlGenerator,
        );
        let indexes = build_loaded(
            &EngineFeatureProfile::mysql_like(),
            InspectorTab::Indexes,
            &table,
            DatabaseType::MySQL,
            &TestDdlGenerator,
        );

        match info.section() {
            Some(InspectorSection::Info { rows }) => assert!(!rows.iter().any(|row| {
                matches!(
                    row,
                    InspectorInfoRow::Field {
                        field: InspectorInfoField::Schema,
                        ..
                    }
                )
            })),
            section => panic!("expected info section, got {section:?}"),
        }
        match indexes.section() {
            Some(InspectorSection::Indexes { show_partial, .. }) => assert!(!show_partial),
            section => panic!("expected index section, got {section:?}"),
        }
    }

    #[test]
    fn mysql_trigger_rows_preserve_action_order_and_creation_context() {
        let mut table = table();
        table.triggers[0].action_order = Some(2);
        table.triggers[0].creation_context = Some(TriggerCreationContext {
            sql_mode: Some("STRICT_TRANS_TABLES".to_string()),
            character_set_client: Some("utf8mb4".to_string()),
            collation_connection: Some("utf8mb4_0900_ai_ci".to_string()),
            database_collation: Some("utf8mb4_0900_ai_ci".to_string()),
            created: Some("2026-08-21 10:20:30.00".to_string()),
        });

        let model = build_loaded(
            &EngineFeatureProfile::mysql_like(),
            InspectorTab::Triggers,
            &table,
            DatabaseType::MySQL,
            &TestDdlGenerator,
        );

        match model.section() {
            Some(InspectorSection::Triggers { rows }) => {
                assert_eq!(rows[0].action_order, Some(2));
                assert_eq!(rows[0].creation_context, table.triggers[0].creation_context);
            }
            section => panic!("expected trigger section, got {section:?}"),
        }
    }

    #[test]
    fn each_section_row_count_is_the_scroll_item_count() {
        let table = table();
        let cases = [
            (InspectorTab::Info, 5),
            (InspectorTab::Columns, 1),
            (InspectorTab::Indexes, 1),
            (InspectorTab::ForeignKeys, 1),
            (InspectorTab::Rls, 5),
            (InspectorTab::Triggers, 1),
            (InspectorTab::Ddl, 3),
        ];

        for (tab, expected_rows) in cases {
            let model = build_loaded(
                &EngineFeatureProfile::postgres_like(),
                tab,
                &table,
                DatabaseType::PostgreSQL,
                &TestDdlGenerator,
            );

            assert_eq!(model.active_tab(), tab);
            assert_eq!(model.row_count(), expected_rows, "tab={tab:?}");
            assert_eq!(
                model.section().map_or(0, InspectorSection::row_count),
                expected_rows
            );
        }
    }

    #[test]
    fn empty_and_unavailable_sections_have_no_scrollable_rows() {
        let mut table = table();
        table.columns.clear();
        table.rls = None;

        let empty = build_loaded(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Columns,
            &table,
            DatabaseType::PostgreSQL,
            &TestDdlGenerator,
        );
        assert_eq!(empty.row_count(), 0);
        assert_eq!(empty.empty_state(), Some(InspectorEmptyState::NoColumns));

        let unavailable = build_loaded(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Rls,
            &table,
            DatabaseType::PostgreSQL,
            &TestDdlGenerator,
        );
        assert_eq!(unavailable.row_count(), 0);
        assert_eq!(
            unavailable.unavailable_reason(),
            Some(InspectorUnavailableReason::RlsNotEnabled)
        );
    }

    #[test]
    fn info_rls_and_ddl_use_the_full_inner_panel_height() {
        let table = table();
        let cases = [
            (InspectorTab::Info, 5_usize),
            (InspectorTab::Rls, 5_usize),
            (InspectorTab::Ddl, 3_usize),
        ];

        for (tab, expected_rows) in cases {
            let model = build_loaded(
                &EngineFeatureProfile::postgres_like(),
                tab,
                &table,
                DatabaseType::PostgreSQL,
                &TestDdlGenerator,
            );

            assert_eq!(model.visible_rows(8), 5, "tab={tab:?}");
            assert_eq!(
                model.max_scroll(8),
                expected_rows.saturating_sub(5),
                "tab={tab:?}"
            );
        }
    }

    #[test]
    fn table_sections_reserve_header_and_scroll_indicator_rows() {
        let mut table = table();
        let column = table.columns[0].clone();
        table.columns.resize(6, column);
        let model = build_loaded(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Columns,
            &table,
            DatabaseType::PostgreSQL,
            &TestDdlGenerator,
        );

        assert_eq!(model.visible_rows(8), 3);
        assert_eq!(model.max_scroll(8), 3);
    }

    #[test]
    fn index_details_are_kept_per_row_when_detail_columns_are_mixed() {
        let mut table = table();
        table.indexes = vec![
            Index {
                name: "users_partial_idx".to_string(),
                columns: vec!["id".to_string()],
                attributes: IndexAttributes::PARTIAL,
                index_type: IndexType::BTree,
                definition: Some(
                    "CREATE INDEX users_partial_idx ON users (id) WHERE id > 0".to_string(),
                ),
            },
            Index {
                name: "users_plain_idx".to_string(),
                columns: vec!["id".to_string()],
                attributes: IndexAttributes::empty(),
                index_type: IndexType::BTree,
                definition: None,
            },
        ];

        let model = build_loaded(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Indexes,
            &table,
            DatabaseType::PostgreSQL,
            &TestDdlGenerator,
        );

        match model.section() {
            Some(InspectorSection::Indexes {
                rows, show_details, ..
            }) => {
                assert!(*show_details);
                assert!(rows[0].detail.is_some());
                assert_eq!(rows[1].detail, None);
            }
            section => panic!("expected index section, got {section:?}"),
        }
    }

    #[test]
    fn mysql_index_rows_show_prefix_direction_and_visibility() {
        let mut table = table();
        table.indexes = vec![
            Index {
                name: "users_email_idx".to_string(),
                columns: vec!["email(8) DESC".to_string()],
                attributes: IndexAttributes::DESCENDING | IndexAttributes::INVISIBLE,
                index_type: IndexType::BTree,
                definition: None,
            },
            Index {
                name: "users_email_functional_idx".to_string(),
                columns: vec!["lower(email)".to_string()],
                attributes: IndexAttributes::EXPRESSION | IndexAttributes::INVISIBLE,
                index_type: IndexType::BTree,
                definition: Some("lower(email)".to_string()),
            },
        ];

        let model = build_loaded(
            &EngineFeatureProfile::mysql_like(),
            InspectorTab::Indexes,
            &table,
            DatabaseType::MySQL,
            &TestDdlGenerator,
        );

        match model.section() {
            Some(InspectorSection::Indexes {
                rows, show_details, ..
            }) => {
                assert!(*show_details);
                assert_eq!(rows[0].columns, "email(8) DESC");
                assert_eq!(rows[0].detail.as_deref(), Some("descending; invisible"));
                assert_eq!(rows[1].detail.as_deref(), Some("lower(email); invisible"));
            }
            section => panic!("expected index section, got {section:?}"),
        }
    }
}
