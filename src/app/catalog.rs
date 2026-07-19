use unicode_width::UnicodeWidthStr;

use crate::model::app_state::AppState;
use crate::model::connection::setup::ConnectionField;
use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::help::{HelpOrigin, JsonbHelpMode, SqlHelpMode};
use crate::model::shared::settings::KeymapPreset;
use crate::policy::{FeaturePolicy, FeatureRequirement};
#[allow(
    clippy::wildcard_imports,
    reason = "help catalog enumerates nearly every keybindings table; explicit list is churn"
)]
use crate::update::input::keybindings::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpDocument {
    filter: String,
    filter_cursor: usize,
    sections: Vec<HelpSection>,
}

const FILTER_LINE_COUNT: usize = 1;
const FILTER_SECTION_GAP_LINES: usize = 1;
const SECTION_HEADER_LINE_COUNT: usize = 1;

impl HelpDocument {
    pub fn from_state(state: &AppState) -> Self {
        let filter = state.ui.help().filter();
        Self::new_with_cursor_and_preset(
            state.ui.help().origin(),
            filter.content(),
            filter.cursor(),
            state.settings.saved_keymap_preset(),
            state.session.active_engine_feature_profile(),
        )
    }

    pub fn new(origin: HelpOrigin, filter: &str) -> Self {
        Self::new_with_cursor(origin, filter, filter.chars().count())
    }

    pub fn new_with_cursor(origin: HelpOrigin, filter: &str, filter_cursor: usize) -> Self {
        Self::new_with_cursor_and_preset(
            origin,
            filter,
            filter_cursor,
            origin.keymap_preset(),
            &EngineFeatureProfile::postgres_like(),
        )
    }

    fn new_with_cursor_and_preset(
        origin: HelpOrigin,
        filter: &str,
        filter_cursor: usize,
        keymap_preset: KeymapPreset,
        engine_feature_profile: &EngineFeatureProfile,
    ) -> Self {
        let feature_policy = FeaturePolicy::new(engine_feature_profile);
        let normalized = filter.trim().to_lowercase();
        let mut sections = vec![current_section(origin, &feature_policy)];
        sections.extend(reference_sections(keymap_preset, &feature_policy));

        if !normalized.is_empty() {
            sections = sections
                .into_iter()
                .filter_map(|section| section.filtered(&normalized))
                .collect();
        }

        if sections.is_empty() {
            sections.push(HelpSection {
                title: "No matches".to_string(),
                rows: vec![HelpRow::new("", "Try another filter")],
            });
        }

        Self {
            filter: filter.to_string(),
            filter_cursor: filter_cursor.min(filter.chars().count()),
            sections,
        }
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn filter_cursor(&self) -> usize {
        self.filter_cursor
    }

    pub fn sections(&self) -> &[HelpSection] {
        &self.sections
    }

    pub fn line_count(&self) -> usize {
        let section_lines: usize = self
            .sections
            .iter()
            .map(|section| SECTION_HEADER_LINE_COUNT + section.rows.len())
            .sum();
        FILTER_LINE_COUNT
            + FILTER_SECTION_GAP_LINES
            + section_lines
            + self.sections.len().saturating_sub(1)
    }

    pub fn content_width(&self) -> usize {
        let filter_label = "Filter: ";
        let filter_width =
            UnicodeWidthStr::width(filter_label) + UnicodeWidthStr::width(self.filter.as_str()) + 1;
        let key_column_width = self.key_column_width();
        self.sections
            .iter()
            .flat_map(|section| {
                let title_width =
                    UnicodeWidthStr::width("▸ ") + UnicodeWidthStr::width(section.title.as_str());
                std::iter::once(title_width)
                    .chain(section.rows.iter().map(|row| row.width(key_column_width)))
            })
            .chain(std::iter::once(filter_width))
            .max()
            .unwrap_or(0)
    }

    pub fn key_column_width(&self) -> usize {
        self.sections
            .iter()
            .flat_map(|section| section.rows.iter())
            .map(HelpRow::key_width)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpSection {
    title: String,
    rows: Vec<HelpRow>,
}

impl HelpSection {
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn rows(&self) -> &[HelpRow] {
        &self.rows
    }

    fn filtered(self, normalized_filter: &str) -> Option<Self> {
        if self.title.to_lowercase().contains(normalized_filter) {
            return Some(self);
        }

        let rows = self
            .rows
            .into_iter()
            .filter(|row| row.matches(normalized_filter))
            .collect::<Vec<_>>();

        if rows.is_empty() {
            None
        } else {
            Some(Self {
                title: self.title,
                rows,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpRow {
    key: String,
    description: String,
}

impl HelpRow {
    fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    fn matches(&self, normalized_filter: &str) -> bool {
        self.key.to_lowercase().contains(normalized_filter)
            || self.description.to_lowercase().contains(normalized_filter)
    }

    pub fn key_width(&self) -> usize {
        UnicodeWidthStr::width(self.key.as_str())
    }

    fn width(&self, key_column_width: usize) -> usize {
        HELP_KEY_INDENT_WIDTH
            + key_column_width
            + HELP_KEY_DESC_GAP
            + UnicodeWidthStr::width(self.description.as_str())
    }
}

fn current_section(origin: HelpOrigin, feature_policy: &FeaturePolicy) -> HelpSection {
    let rows = match origin {
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Result,
            staged_delete_in_progress: true,
            result_active: true,
            ..
        } => rows_from_binding_refs(&[
            &result_active::STAGE_DELETE,
            &result_active::UNSTAGE_DELETE,
            &cell_edit::WRITE,
            &result_active::ESC_BACK,
        ]),
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Result,
            staged_delete_in_progress: true,
            result_active: false,
            keymap_preset,
            ..
        } => rows_from_binding_refs(&[
            &result_active::ENTER_DEEPEN,
            &result_active::UNSTAGE_DELETE,
            &cell_edit::WRITE,
            &footer_nav::PAGE_NAV,
            csv_export(keymap_preset),
        ]),
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Result,
            result_active: true,
            staged_delete_in_progress: false,
            can_write_preview,
            ..
        } => {
            let mut rows = vec![
                &result_active::YANK,
                &result_active::ROW_YANK,
                &result_active::ROW_DETAIL,
            ];
            if can_write_preview {
                rows.push(&result_active::STAGE_DELETE);
                rows.push(&result_active::EDIT);
            }
            rows.push(&result_active::ESC_BACK);
            rows_from_binding_refs(&rows)
        }
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Result,
            keymap_preset,
            ..
        } => rows_from_binding_refs(&[
            &result_active::ENTER_DEEPEN,
            &footer_nav::PAGE_NAV,
            csv_export(keymap_preset),
        ]),
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Inspector,
            ..
        } => rows_from_binding_refs(&[&global::INSPECTOR_TABS, &inspector_ddl::YANK]),
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Explorer,
            keymap_preset,
            ..
        } => rows_from_binding_refs(&[
            table_picker(keymap_preset),
            &global::CONNECTIONS,
            &global::SQL,
        ]),
        HelpOrigin::CommandLine => command_line_rows(feature_policy),
        HelpOrigin::CellEdit => rows_from_bindings(CELL_EDIT_KEYS),
        HelpOrigin::TablePicker => rows_from_mode_rows(TABLE_PICKER_ROWS),
        HelpOrigin::CommandPalette => rows_from_mode_rows(COMMAND_PALETTE_ROWS),
        HelpOrigin::Settings => rows_from_mode_rows(SETTINGS_ROWS),
        HelpOrigin::Help => rows_from_mode_rows(HELP_ROWS),
        HelpOrigin::SqlModal {
            mode,
            keymap_preset,
        } => sql_current_rows(mode, keymap_preset, feature_policy),
        HelpOrigin::ConnectionSetup {
            keymap_preset,
            focused_field,
        } => connection_setup_current_rows(keymap_preset, focused_field),
        HelpOrigin::ConnectionError => rows_from_mode_rows(CONNECTION_ERROR_ROWS),
        HelpOrigin::SqliteDiagnostics => {
            rows_from_mode_rows_if_visible(SQLITE_DIAGNOSTICS_ROWS, feature_policy)
        }
        HelpOrigin::ConfirmDialog => rows_from_bindings(CONFIRM_DIALOG_KEYS),
        HelpOrigin::ConnectionSelector => rows_from_mode_rows(CONNECTION_SELECTOR_ROWS),
        HelpOrigin::ErTablePicker { keymap_preset } => {
            rows_from_mode_rows_if_visible(er_picker_rows(keymap_preset), feature_policy)
        }
        HelpOrigin::QueryHistoryPicker => rows_from_mode_rows(QUERY_HISTORY_PICKER_ROWS),
        HelpOrigin::JsonbDetail { mode } => jsonb_current_rows(mode, feature_policy),
        HelpOrigin::JsonbEdit => rows_from_mode_rows_if_visible(JSONB_EDIT_ROWS, feature_policy),
        HelpOrigin::CellDetail { searching: true } => rows_from_bindings(CELL_DETAIL_SEARCH_KEYS),
        HelpOrigin::CellDetail { searching: false } => rows_from_mode_rows(CELL_DETAIL_ROWS),
        HelpOrigin::RowDetail => rows_from_mode_rows(ROW_DETAIL_ROWS),
    };

    HelpSection {
        title: format!("Current: {}", origin.label()),
        rows,
    }
}

fn command_line_rows(feature_policy: &FeaturePolicy) -> Vec<HelpRow> {
    rows_from_binding_iter(
        COMMAND_LINE_KEYS
            .iter()
            .filter(|binding| feature_policy.is_visible(binding.feature_requirement())),
    )
}

fn reference_sections(
    keymap_preset: KeymapPreset,
    feature_policy: &FeaturePolicy,
) -> Vec<HelpSection> {
    let mut open_switch_rows = vec![
        table_picker(keymap_preset),
        &global::SQL,
        &global::CONNECTIONS,
        query_history(keymap_preset),
        &global::PANE_SWITCH,
        &global::INSPECTOR_TABS,
    ];
    if feature_policy.is_visible(global::ER_DIAGRAM.feature_requirement()) {
        open_switch_rows.insert(2, &global::ER_DIAGRAM);
    }
    if feature_policy.is_visible(sqlite_diagnostics(keymap_preset).feature_requirement()) {
        open_switch_rows.insert(2, sqlite_diagnostics(keymap_preset));
    }

    let mut data_action_rows = rows_from_binding_refs(&[
        &global::RELOAD,
        csv_export(keymap_preset),
        &result_active::YANK,
        &result_active::ROW_YANK,
        &result_active::STAGE_DELETE,
        &result_active::UNSTAGE_DELETE,
        &inspector_ddl::YANK,
    ]);
    if feature_policy.is_visible(jsonb_detail::YANK.feature_requirement()) {
        data_action_rows.extend(rows_from_mode_row_refs_if_visible(
            &[&jsonb_detail::YANK],
            feature_policy,
        ));
    }

    let mut search_filter_rows = rows_from_mode_row_refs(&[
        &table_picker::TYPE_FILTER,
        &query_history_picker::TYPE_FILTER,
    ]);
    if feature_policy.is_visible(er_picker::TYPE_FILTER.feature_requirement()) {
        search_filter_rows.insert(1, row_from_mode_row(&er_picker::TYPE_FILTER));
    }
    if feature_policy.is_visible(jsonb_search::TYPE_SEARCH.feature_requirement()) {
        search_filter_rows.extend(rows_from_bindings_if_visible(
            JSONB_SEARCH_KEYS,
            feature_policy,
        ));
    }
    search_filter_rows.extend(rows_from_mode_rows(HELP_ROWS));

    let mut editing_rows = merge_rows(&[
        sql_current_rows(SqlHelpMode::Normal, keymap_preset, feature_policy),
        sql_current_rows(SqlHelpMode::Insert, keymap_preset, feature_policy),
        rows_from_bindings(CELL_EDIT_KEYS),
        rows_from_bindings_if_visible(SQL_MODAL_CONFIRMING_KEYS, feature_policy),
    ]);
    if feature_policy.is_visible(jsonb_edit::ESC_NORMAL.feature_requirement()) {
        editing_rows.extend(rows_from_mode_rows_if_visible(
            JSONB_EDIT_ROWS,
            feature_policy,
        ));
    }

    let mut advanced_rows = Vec::new();
    if feature_policy.is_visible(FeatureRequirement::Explain) {
        advanced_rows.extend(sql_current_rows(
            SqlHelpMode::Plan,
            keymap_preset,
            feature_policy,
        ));
    }
    if feature_policy.is_visible(FeatureRequirement::PlanComparison) {
        advanced_rows.extend(sql_current_rows(
            SqlHelpMode::Compare,
            keymap_preset,
            feature_policy,
        ));
    }
    if feature_policy.is_visible(FeatureRequirement::ErDiagram) {
        advanced_rows.extend(rows_from_mode_rows_if_visible(
            er_picker_rows(keymap_preset),
            feature_policy,
        ));
    }
    if feature_policy.is_visible(FeatureRequirement::JsonbDetail) {
        advanced_rows.extend(rows_from_mode_rows_if_visible(
            JSONB_DETAIL_ROWS,
            feature_policy,
        ));
    }
    advanced_rows.extend(rows_from_mode_rows(ROW_DETAIL_ROWS));

    vec![
        section(
            "Common",
            rows_from_binding_refs(&[
                &global::HELP,
                &global::QUIT,
                settings(keymap_preset),
                command_palette(keymap_preset),
                &global::COMMAND_LINE,
                &global::FOCUS,
                read_only(keymap_preset),
            ]),
        ),
        section("Navigation", rows_from_bindings(NAVIGATION_KEYS)),
        section("Open / Switch", rows_from_binding_refs(&open_switch_rows)),
        section("Data Actions", data_action_rows),
        section("Editing", editing_rows),
        section("Search / Filter", search_filter_rows),
        section(
            "Connections",
            merge_rows(&[
                rows_from_binding_refs(&[
                    &connection_setup::TAB_NAV,
                    &connection_setup::TAB_NEXT,
                    &connection_setup::TAB_PREV,
                    connection_setup_save(keymap_preset),
                    &connection_setup::ESC_CANCEL,
                    &connection_setup::ENTER_DROPDOWN,
                    &connection_setup::DROPDOWN_NAV,
                ]),
                rows_from_mode_rows(CONNECTION_SELECTOR_ROWS),
                rows_from_mode_rows(CONNECTION_ERROR_ROWS),
            ]),
        ),
        section(
            "Modal Basics",
            merge_rows(&[
                rows_from_bindings(OVERLAY_KEYS),
                rows_from_bindings(CONFIRM_DIALOG_KEYS),
                rows_from_mode_rows(HELP_ROWS),
            ]),
        ),
        section("Advanced", merge_rows(&[advanced_rows])),
    ]
    .into_iter()
    .filter(|section| !section.rows.is_empty())
    .collect()
}

fn sql_current_rows(
    mode: SqlHelpMode,
    keymap_preset: KeymapPreset,
    feature_policy: &FeaturePolicy,
) -> Vec<HelpRow> {
    match mode {
        SqlHelpMode::Normal => rows_from_binding_refs_if_visible(
            &[
                &sql_modal_normal::RUN,
                &sql_modal_normal::YANK,
                &sql_modal_normal::ENTER_INSERT,
                &sql_modal_normal::APPEND,
                &sql_modal_normal::MOVE,
                &sql_modal_normal::HOME_END,
                &sql_modal_normal::VIEWPORT,
                &sql_modal_normal::CLOSE,
                &sql_modal_normal::CLEAR,
                sql_modal_normal_query_history(keymap_preset),
            ],
            feature_policy,
        ),
        SqlHelpMode::Insert | SqlHelpMode::Running => match keymap_preset {
            KeymapPreset::Default => rows_from_bindings_if_visible(SQL_MODAL_KEYS, feature_policy),
            KeymapPreset::Ide => rows_from_binding_refs_if_visible(
                &[
                    &sql_modal::RUN,
                    &sql_modal::ESC_NORMAL,
                    &sql_modal::MOVE,
                    &sql_modal::HOME_END,
                    &sql_modal::TAB,
                    &sql_modal::CLEAR,
                ],
                feature_policy,
            ),
        },
        SqlHelpMode::Plan => {
            let mut bindings: Vec<&KeyBinding> = vec![sql_modal_plan_explain(keymap_preset)];
            if feature_policy.is_visible(sql_modal_plan::ANALYZE.feature_requirement()) {
                bindings.push(&sql_modal_plan::ANALYZE);
            }
            bindings.extend([
                &sql_modal_plan::YANK,
                &sql_modal_plan::SCROLL,
                &sql_modal_plan::TAB,
                &sql_modal_plan::BACKTAB,
                &sql_modal_plan::CLOSE,
            ]);
            rows_from_binding_refs_if_visible(&bindings, feature_policy)
        }
        SqlHelpMode::Compare => {
            let mut bindings: Vec<&KeyBinding> = vec![sql_modal_compare_explain(keymap_preset)];
            if feature_policy.is_visible(sql_modal_compare::ANALYZE.feature_requirement()) {
                bindings.push(&sql_modal_compare::ANALYZE);
            }
            bindings.extend([
                &sql_modal_compare::EDIT_QUERY,
                &sql_modal_compare::YANK,
                &sql_modal_compare::SCROLL,
                &sql_modal_compare::TAB,
                &sql_modal_compare::BACKTAB,
                &sql_modal_compare::CLOSE,
            ]);
            rows_from_binding_refs_if_visible(&bindings, feature_policy)
        }
        SqlHelpMode::Confirm => {
            rows_from_bindings_if_visible(SQL_MODAL_CONFIRMING_KEYS, feature_policy)
        }
    }
}

fn connection_setup_current_rows(
    keymap_preset: KeymapPreset,
    focused_field: ConnectionField,
) -> Vec<HelpRow> {
    let is_dropdown_field = matches!(
        focused_field,
        ConnectionField::DatabaseType | ConnectionField::SslMode
    );
    let submit = if is_dropdown_field {
        &connection_setup::ENTER_DROPDOWN
    } else {
        connection_setup_save(keymap_preset)
    };
    let mut rows = rows_from_binding_refs(&[
        &connection_setup::TAB_NAV,
        &connection_setup::TAB_NEXT,
        &connection_setup::TAB_PREV,
        submit,
    ]);
    if is_dropdown_field {
        rows.push(HelpRow::new(
            connection_setup::SAVE.key,
            connection_setup::SAVE.description,
        ));
    }
    rows.extend(rows_from_binding_refs(&[
        &connection_setup::ESC_CANCEL,
        &connection_setup::DROPDOWN_NAV,
    ]));
    rows
}

fn jsonb_current_rows(mode: JsonbHelpMode, feature_policy: &FeaturePolicy) -> Vec<HelpRow> {
    match mode {
        JsonbHelpMode::Detail => rows_from_mode_rows_if_visible(JSONB_DETAIL_ROWS, feature_policy),
        JsonbHelpMode::Search => rows_from_bindings_if_visible(JSONB_SEARCH_KEYS, feature_policy),
        JsonbHelpMode::Edit => rows_from_mode_rows_if_visible(JSONB_EDIT_ROWS, feature_policy),
    }
}

fn section(title: &str, rows: Vec<HelpRow>) -> HelpSection {
    HelpSection {
        title: title.to_string(),
        rows,
    }
}

fn rows_from_bindings(bindings: &[KeyBinding]) -> Vec<HelpRow> {
    rows_from_binding_iter(bindings.iter())
}

fn rows_from_bindings_if_visible(
    bindings: &[KeyBinding],
    feature_policy: &FeaturePolicy,
) -> Vec<HelpRow> {
    rows_from_binding_iter(
        bindings
            .iter()
            .filter(|binding| feature_policy.is_visible(binding.feature_requirement())),
    )
}

fn rows_from_binding_refs(bindings: &[&KeyBinding]) -> Vec<HelpRow> {
    rows_from_binding_iter(bindings.iter().copied())
}

fn rows_from_binding_refs_if_visible(
    bindings: &[&KeyBinding],
    feature_policy: &FeaturePolicy,
) -> Vec<HelpRow> {
    rows_from_binding_iter(
        bindings
            .iter()
            .copied()
            .filter(|binding| feature_policy.is_visible(binding.feature_requirement())),
    )
}

fn rows_from_binding_iter<'a>(bindings: impl IntoIterator<Item = &'a KeyBinding>) -> Vec<HelpRow> {
    let bindings = bindings.into_iter().collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut i = 0;
    while i < bindings.len() {
        if i + 1 < bindings.len()
            && let Some(row) = paired_binding_row(bindings[i], bindings[i + 1])
        {
            rows.push(row);
            i += 2;
        } else if i + 1 < bindings.len() && bindings[i].key == bindings[i + 1].key {
            rows.push(HelpRow::new(
                bindings[i].key,
                format!("Toggle {}", bindings[i].desc_short),
            ));
            i += 2;
        } else {
            rows.push(HelpRow::new(bindings[i].key, bindings[i].description));
            i += 1;
        }
    }
    rows
}

fn paired_binding_row(first: &KeyBinding, second: &KeyBinding) -> Option<HelpRow> {
    let pair = (first.desc_short, second.desc_short);
    let (description, separator) = match pair {
        ("Down", "Up") if first.description.starts_with("Move") => {
            ("Move down / up / scroll", " / ")
        }
        ("Down", "Up") => ("Scroll down / up", " / "),
        ("Top", "Bottom") => ("Jump to top / bottom", " / "),
        ("Next Page", "Prev Page") => ("Next / previous page", " / "),
        _ => return None,
    };

    Some(HelpRow::new(
        format!("{}{}{}", first.key, separator, second.key),
        description,
    ))
}

fn rows_from_mode_rows(rows: &[ModeRow]) -> Vec<HelpRow> {
    rows.iter().map(row_from_mode_row).collect()
}

fn rows_from_mode_rows_if_visible(
    rows: &[ModeRow],
    feature_policy: &FeaturePolicy,
) -> Vec<HelpRow> {
    rows.iter()
        .filter(|row| feature_policy.is_visible(row.feature_requirement()))
        .map(row_from_mode_row)
        .collect()
}

fn rows_from_mode_row_refs(rows: &[&ModeRow]) -> Vec<HelpRow> {
    rows.iter().map(|&row| row_from_mode_row(row)).collect()
}

fn rows_from_mode_row_refs_if_visible(
    rows: &[&ModeRow],
    feature_policy: &FeaturePolicy,
) -> Vec<HelpRow> {
    rows.iter()
        .filter(|row| feature_policy.is_visible(row.feature_requirement()))
        .map(|&row| row_from_mode_row(row))
        .collect()
}

fn row_from_mode_row(row: &ModeRow) -> HelpRow {
    HelpRow::new(row.key, row.description)
}

fn merge_rows(groups: &[Vec<HelpRow>]) -> Vec<HelpRow> {
    let mut rows = Vec::new();
    for group in groups {
        for row in group {
            if !rows.iter().any(|existing: &HelpRow| {
                existing.key == row.key && existing.description == row.description
            }) {
                rows.push(row.clone());
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConnectionId, DatabaseType};
    use crate::model::browse::jsonb_detail::JsonbDetailMode;
    use crate::model::shared::input_mode::InputMode;
    use crate::model::sql_editor::modal::SqlModalTab;

    fn row_descriptions(document: &HelpDocument) -> Vec<&str> {
        document
            .sections()
            .iter()
            .flat_map(HelpSection::rows)
            .map(HelpRow::description)
            .collect()
    }

    #[test]
    fn document_starts_with_current_section() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: true,
                staged_delete_in_progress: false,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "",
        );

        assert_eq!(document.sections()[0].title(), "Current: Result Pane");
        assert!(
            document.sections()[0]
                .rows()
                .iter()
                .any(|row| row.description().contains("active row"))
        );
    }

    #[test]
    fn result_scroll_help_omits_row_detail() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: false,
                staged_delete_in_progress: false,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "",
        );

        assert!(
            !document.sections()[0]
                .rows()
                .iter()
                .any(|row| row.description() == "Open Row Detail")
        );
    }

    #[test]
    fn result_active_help_includes_row_detail() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: true,
                staged_delete_in_progress: false,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "",
        );

        assert!(
            document.sections()[0]
                .rows()
                .iter()
                .any(|row| row.description() == "Open Row Detail")
        );
    }

    #[test]
    fn read_only_result_active_help_omits_write_actions() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: true,
                staged_delete_in_progress: false,
                can_write_preview: false,
                keymap_preset: KeymapPreset::default(),
            },
            "",
        );
        let current_rows = document.sections()[0].rows();

        assert!(!current_rows.iter().any(|row| row.description()
            == "Stage the active row for deletion (red highlight; :w to commit)"));
        assert!(
            !current_rows
                .iter()
                .any(|row| row.description() == "Edit active cell")
        );
        assert!(
            current_rows
                .iter()
                .any(|row| row.description() == "Open Row Detail")
        );
    }

    #[test]
    fn staged_delete_result_help_omits_disabled_result_actions() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: true,
                staged_delete_in_progress: true,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "",
        );
        let current_rows = document.sections()[0].rows();

        assert!(
            !current_rows
                .iter()
                .any(|row| row.description() == "Open Row Detail")
        );
        assert!(
            !current_rows
                .iter()
                .any(|row| row.description() == "Copy the active cell value to clipboard")
        );
        assert!(
            current_rows
                .iter()
                .any(|row| row.description() == "Unstage the last staged row deletion")
        );
    }

    #[test]
    fn staged_delete_result_scroll_help_includes_unstage() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: false,
                staged_delete_in_progress: true,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "",
        );
        let current_rows = document.sections()[0].rows();

        assert!(
            current_rows
                .iter()
                .any(|row| row.description() == "Unstage the last staged row deletion")
        );
        assert!(
            !current_rows
                .iter()
                .any(|row| row.description() == "Open Row Detail")
        );
    }

    #[test]
    fn filter_matches_key_description_and_section_title() {
        let copy = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: true,
                staged_delete_in_progress: false,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "copy",
        );
        let copy_rows = copy
            .sections()
            .iter()
            .flat_map(HelpSection::rows)
            .collect::<Vec<_>>();

        assert!(!copy_rows.is_empty());
        assert!(
            copy_rows
                .iter()
                .all(|row| row.key().to_lowercase().contains("copy")
                    || row.description().to_lowercase().contains("copy"))
        );

        let navigation = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Explorer,
                result_active: false,
                staged_delete_in_progress: false,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "navigation",
        );

        assert!(
            navigation
                .sections()
                .iter()
                .any(|section| section.title() == "Navigation" && section.rows().len() > 1)
        );
    }

    #[test]
    fn no_matches_document_has_fallback_row() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Explorer,
                result_active: false,
                staged_delete_in_progress: false,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "zz-no-match",
        );

        assert_eq!(document.sections()[0].title(), "No matches");
        assert_eq!(
            document.sections()[0].rows()[0].description(),
            "Try another filter"
        );
    }

    #[test]
    fn line_count_uses_rendered_section_dimensions() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Explorer,
                result_active: false,
                staged_delete_in_progress: false,
                can_write_preview: true,
                keymap_preset: KeymapPreset::default(),
            },
            "",
        );
        let section_lines: usize = document
            .sections()
            .iter()
            .map(|section| SECTION_HEADER_LINE_COUNT + section.rows().len())
            .sum();
        let expected = FILTER_LINE_COUNT
            + FILTER_SECTION_GAP_LINES
            + section_lines
            + document.sections().len().saturating_sub(1);

        assert_eq!(document.line_count(), expected);
    }

    #[test]
    fn content_width_expands_to_longest_key_label() {
        let document = HelpDocument::new(HelpOrigin::Help, "");

        assert!(
            document.key_column_width()
                >= UnicodeWidthStr::width("Ctrl+F / Ctrl+B / PageDown / PageUp")
        );
    }

    #[test]
    fn document_preserves_filter_cursor() {
        let document = HelpDocument::new_with_cursor(HelpOrigin::Help, "copy", 2);

        assert_eq!(document.filter(), "copy");
        assert_eq!(document.filter_cursor(), 2);
    }

    #[test]
    fn origin_from_state_maps_sql_plan_and_jsonb_search() {
        let mut sql_state = AppState::new("test".to_string());
        sql_state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/test",
        );
        sql_state.modal.set_mode(InputMode::SqlModal);
        sql_state.sql_modal.set_active_tab(SqlModalTab::Plan);
        let sql_document = HelpDocument::new(HelpOrigin::from_state(&sql_state), "");

        assert_eq!(
            sql_document.sections()[0].title(),
            "Current: SQL Editor Plan"
        );

        let mut jsonb_state = AppState::new("test".to_string());
        jsonb_state.modal.set_mode(InputMode::JsonbDetail);
        jsonb_state
            .jsonb_detail
            .set_mode(JsonbDetailMode::Searching);
        let jsonb_document = HelpDocument::new(HelpOrigin::from_state(&jsonb_state), "");

        assert_eq!(
            jsonb_document.sections()[0].title(),
            "Current: JSONB Search"
        );
    }

    #[test]
    fn from_state_omits_postgresql_only_reference_rows_for_sqlite() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::SQLite,
            "sqlite://test.db",
        );
        let origin = HelpOrigin::from_state(&state);
        state.ui.help_mut().open(origin);

        let document = HelpDocument::from_state(&state);
        let descriptions = row_descriptions(&document);

        assert!(
            !descriptions
                .iter()
                .any(|description| description.contains("ER Diagram"))
        );
        assert!(
            descriptions
                .iter()
                .any(|description| description.contains("EXPLAIN"))
        );
        assert!(
            !descriptions
                .iter()
                .any(|description| description.contains("Compare"))
        );
        assert!(
            !descriptions
                .iter()
                .any(|description| description.contains("ANALYZE"))
        );
        assert!(
            !descriptions
                .iter()
                .any(|description| description.contains("JSONB"))
        );
    }

    #[test]
    fn sqlite_sql_help_origin_includes_plan_tab() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::SQLite,
            "sqlite://test.db",
        );
        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.set_active_tab(SqlModalTab::Plan);

        let document = HelpDocument::new(HelpOrigin::from_state(&state), "");

        assert_eq!(document.sections()[0].title(), "Current: SQL Editor Plan");
    }

    #[test]
    fn sqlite_command_line_help_omits_erd_command() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::SQLite,
            "sqlite://test.db",
        );
        state.modal.set_mode(InputMode::CommandLine);
        let origin = HelpOrigin::from_state(&state);
        state.ui.help_mut().open(origin);

        let document = HelpDocument::from_state(&state);

        assert_eq!(document.sections()[0].title(), "Current: Command Line");
        assert!(
            !document.sections()[0]
                .rows()
                .iter()
                .any(|row| row.key() == ":erd")
        );
    }

    #[test]
    fn connection_setup_help_matches_ssl_field_actions() {
        let document = HelpDocument::new(
            HelpOrigin::ConnectionSetup {
                keymap_preset: KeymapPreset::Ide,
                focused_field: ConnectionField::SslMode,
            },
            "",
        );
        let current_rows = document.sections()[0].rows();

        assert!(current_rows.iter().any(|row| {
            row.key() == connection_setup::ENTER_DROPDOWN.key
                && row.description() == connection_setup::ENTER_DROPDOWN.description
        }));
        assert!(current_rows.iter().any(|row| {
            row.key() == connection_setup::SAVE.key
                && row.description() == connection_setup::SAVE.description
        }));
        assert!(!current_rows.iter().any(|row| {
            row.key() == connection_setup::SAVE_IDE.key
                && row.description() == connection_setup::SAVE_IDE.description
        }));
    }
}
