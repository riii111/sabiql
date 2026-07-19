use crate::domain::{DatabaseType, ForeignKey, Index, IndexType, RlsInfo, Table};
use crate::model::shared::engine_feature_profile::{EngineFeatureProfile, InspectorInfoField};
use crate::model::shared::inspector_tab::InspectorTab;
use crate::policy::table_kind::{inspector_flags_label, inspector_kind_label};
use crate::ports::outbound::DdlGenerator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorViewModel {
    active_tab: InspectorTab,
    section: Option<InspectorSection>,
    empty_state: Option<InspectorEmptyState>,
    unavailable_reason: Option<InspectorUnavailableReason>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorTriggerRow {
    pub name: String,
    pub timing: String,
    pub events: String,
    pub function_name: String,
    pub security_definer: bool,
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
    pub fn build(
        profile: &EngineFeatureProfile,
        selected_tab: InspectorTab,
        table: Option<&Table>,
        database_type: DatabaseType,
        ddl_generator: &dyn DdlGenerator,
    ) -> Self {
        let active_tab = profile.normalize_inspector_tab(selected_tab);
        let Some(table) = table else {
            return Self {
                active_tab,
                section: None,
                empty_state: Some(InspectorEmptyState::NoTableSelected),
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
                        detail: show_details.then(|| index_detail(index)),
                    })
                    .collect();
                (
                    InspectorSection::Indexes {
                        rows,
                        show_type,
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
                        function_name: trigger.function_name.clone(),
                        security_definer: trigger.security_definer,
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
            section: Some(section),
            empty_state,
            unavailable_reason,
        }
    }

    pub fn active_tab(&self) -> InspectorTab {
        self.active_tab
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
        return definition.clone();
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
                on_delete: FkAction::NoAction,
                on_update: FkAction::NoAction,
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
                function_name: "set_updated_at".to_string(),
                security_definer: false,
            }],
            row_count_estimate: Some(3),
            comment: Some("Users".to_string()),
            source_ddl: None,
            kind_info: TableKindInfo::default(),
        }
    }

    #[test]
    fn no_table_exposes_empty_state_without_display_rows() {
        let model = InspectorViewModel::build(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Info,
            None,
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
            let model = InspectorViewModel::build(
                &EngineFeatureProfile::postgres_like(),
                tab,
                Some(&table),
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

        let empty = InspectorViewModel::build(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Columns,
            Some(&table),
            DatabaseType::PostgreSQL,
            &TestDdlGenerator,
        );
        assert_eq!(empty.row_count(), 0);
        assert_eq!(empty.empty_state(), Some(InspectorEmptyState::NoColumns));

        let unavailable = InspectorViewModel::build(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Rls,
            Some(&table),
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
            let model = InspectorViewModel::build(
                &EngineFeatureProfile::postgres_like(),
                tab,
                Some(&table),
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
        let model = InspectorViewModel::build(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Columns,
            Some(&table),
            DatabaseType::PostgreSQL,
            &TestDdlGenerator,
        );

        assert_eq!(model.visible_rows(8), 3);
        assert_eq!(model.max_scroll(8), 3);
    }
}
