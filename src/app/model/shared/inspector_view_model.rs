use crate::domain::{DatabaseType, ForeignKey, Index, IndexType, RlsInfo, Table};
use crate::model::shared::engine_feature_profile::{EngineFeatureProfile, InspectorInfoField};
use crate::model::shared::inspector_tab::InspectorTab;
use crate::policy::table_kind::{inspector_flags_label, inspector_kind_label};
use crate::ports::outbound::DdlGenerator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorViewModel {
    active_tab: InspectorTab,
    sections: Vec<InspectorSection>,
    empty_state: Option<InspectorEmptyState>,
    unavailable_reason: Option<InspectorUnavailableReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorSection {
    Info {
        rows: Vec<InspectorDisplayRow>,
    },
    Columns {
        rows: Vec<InspectorDisplayRow>,
        show_read_only: bool,
    },
    Indexes {
        rows: Vec<InspectorDisplayRow>,
        show_type: bool,
        show_details: bool,
    },
    ForeignKeys {
        rows: Vec<InspectorDisplayRow>,
    },
    Rls {
        rows: Vec<InspectorDisplayRow>,
    },
    Triggers {
        rows: Vec<InspectorDisplayRow>,
    },
    Ddl {
        rows: Vec<InspectorDisplayRow>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorDisplayRow {
    Info {
        field: InspectorInfoField,
        value: Option<String>,
    },
    Cells(Vec<String>),
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
    Text(String),
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
                sections: Vec::new(),
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
                        .map(|field| InspectorDisplayRow::Info {
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
                    .map(|column| {
                        let mut cells = vec![
                            column.name.clone(),
                            column.data_type.clone(),
                            if column.is_nullable() {
                                "✓".to_string()
                            } else {
                                String::new()
                            },
                            if column.is_primary_key() {
                                "✓".to_string()
                            } else {
                                String::new()
                            },
                        ];
                        if show_read_only {
                            cells.push(column.read_only_reason().unwrap_or_default().to_string());
                        }
                        cells.push(column.default.clone().unwrap_or_default());
                        cells.push(column.comment.clone().unwrap_or_default());
                        InspectorDisplayRow::Cells(cells)
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
                    .map(|index| {
                        InspectorDisplayRow::Cells(index_row(index, show_type, show_details))
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
                let rows = table
                    .foreign_keys
                    .iter()
                    .map(|fk| InspectorDisplayRow::Cells(foreign_key_row(fk)))
                    .collect();
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
                    .map(|trigger| {
                        InspectorDisplayRow::Cells(vec![
                            trigger.name.clone(),
                            trigger.timing.to_string(),
                            trigger
                                .events
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join("/"),
                            trigger.function_name.clone(),
                            if trigger.security_definer {
                                "✓".to_string()
                            } else {
                                String::new()
                            },
                        ])
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
                        .map(|line| InspectorDisplayRow::Text(line.to_string()))
                        .collect(),
                },
                None,
                None,
            ),
        };

        Self {
            active_tab,
            sections: vec![section],
            empty_state,
            unavailable_reason,
        }
    }

    pub fn active_tab(&self) -> InspectorTab {
        self.active_tab
    }

    pub fn sections(&self) -> &[InspectorSection] {
        &self.sections
    }

    pub fn empty_state(&self) -> Option<InspectorEmptyState> {
        self.empty_state
    }

    pub fn unavailable_reason(&self) -> Option<InspectorUnavailableReason> {
        self.unavailable_reason
    }

    pub fn row_count(&self) -> usize {
        self.sections.iter().map(InspectorSection::row_count).sum()
    }

    pub fn visible_rows(&self, pane_height: u16) -> usize {
        match self.sections.first() {
            Some(InspectorSection::Ddl { .. }) => pane_height.saturating_sub(3) as usize,
            _ => pane_height.saturating_sub(5) as usize,
        }
    }

    pub fn max_scroll(&self, pane_height: u16) -> usize {
        self.row_count()
            .saturating_sub(self.visible_rows(pane_height))
    }
}

impl InspectorSection {
    pub fn rows(&self) -> &[InspectorDisplayRow] {
        match self {
            Self::Info { rows }
            | Self::Columns { rows, .. }
            | Self::Indexes { rows, .. }
            | Self::ForeignKeys { rows }
            | Self::Rls { rows }
            | Self::Triggers { rows }
            | Self::Ddl { rows } => rows,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows().len()
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

fn index_row(index: &Index, show_type: bool, show_details: bool) -> Vec<String> {
    let mut row = vec![index.name.clone(), index.columns.join(", ")];
    if show_type {
        row.push(index_type_label(index));
    }
    row.push(if index.is_unique() {
        "✓".to_string()
    } else {
        String::new()
    });
    if show_details {
        row.push(if index.is_partial() {
            "✓".to_string()
        } else {
            String::new()
        });
        row.push(index_detail(index));
    }
    row
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

fn index_type_label(index: &Index) -> String {
    match index.index_type {
        IndexType::Unknown => String::new(),
        _ => index.index_type.to_string(),
    }
}

fn foreign_key_row(fk: &ForeignKey) -> Vec<String> {
    let references = format!(
        "{}.{}({})",
        fk.to_schema,
        fk.to_table,
        fk.to_columns.join(", ")
    );
    vec![
        fk.name.clone(),
        fk.from_columns.join(", "),
        if fk.is_reference_resolved() {
            references
        } else {
            format!("{references} (unresolved)")
        },
    ]
}

fn rls_rows(rls: &RlsInfo) -> Vec<InspectorDisplayRow> {
    let mut rows = vec![InspectorDisplayRow::RlsStatus {
        enabled: rls.enabled,
        force: rls.force,
    }];
    if !rls.policies.is_empty() {
        rows.push(InspectorDisplayRow::RlsSpacer);
        rows.push(InspectorDisplayRow::RlsPoliciesHeading);
        for policy in &rls.policies {
            rows.push(InspectorDisplayRow::RlsPolicy {
                name: policy.name.clone(),
                command: policy.cmd.to_string(),
                permissive: policy.permissive,
            });
            if let Some(qual) = &policy.qual {
                rows.push(InspectorDisplayRow::RlsPolicyQual(qual.clone()));
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
                model.row_count(),
                model
                    .sections()
                    .iter()
                    .map(InspectorSection::row_count)
                    .sum::<usize>()
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
    fn table_sections_reserve_header_and_scroll_indicator_rows() {
        let model = InspectorViewModel::build(
            &EngineFeatureProfile::postgres_like(),
            InspectorTab::Columns,
            Some(&table()),
            DatabaseType::PostgreSQL,
            &TestDdlGenerator,
        );

        assert_eq!(model.visible_rows(20), 15);
        assert_eq!(model.max_scroll(20), 0);
    }
}
