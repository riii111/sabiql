use unicode_width::UnicodeWidthStr;

use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::help::{HelpOrigin, JsonbHelpMode, SqlHelpMode};
use crate::update::input::keybindings::{
    CELL_EDIT_KEYS, COMMAND_LINE_KEYS, COMMAND_PALETTE_ROWS, CONFIRM_DIALOG_KEYS,
    CONNECTION_ERROR_ROWS, CONNECTION_SELECTOR_ROWS, CONNECTION_SETUP_KEYS, ER_PICKER_ROWS,
    FOOTER_NAV_KEYS, GLOBAL_KEYS, HELP_KEY_DESC_GAP, HELP_KEY_INDENT_WIDTH, HELP_ROWS,
    HISTORY_KEYS, INSPECTOR_DDL_KEYS, JSONB_DETAIL_ROWS, JSONB_EDIT_ROWS, JSONB_SEARCH_KEYS,
    KeyBinding, ModeRow, NAVIGATION_KEYS, OVERLAY_KEYS, QUERY_HISTORY_PICKER_ROWS,
    RESULT_ACTIVE_KEYS, SETTINGS_ROWS, SQL_MODAL_COMPARE_KEYS, SQL_MODAL_CONFIRMING_KEYS,
    SQL_MODAL_KEYS, SQL_MODAL_NORMAL_KEYS, SQL_MODAL_PLAN_KEYS, TABLE_PICKER_ROWS, idx,
};

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
        let filter = state.ui.help.filter();
        Self::new_with_cursor(state.ui.help.origin(), filter.content(), filter.cursor())
    }

    pub fn new(origin: HelpOrigin, filter: &str) -> Self {
        Self::new_with_cursor(origin, filter, filter.chars().count())
    }

    pub fn new_with_cursor(origin: HelpOrigin, filter: &str, filter_cursor: usize) -> Self {
        let normalized = filter.trim().to_lowercase();
        let mut sections = vec![current_section(origin)];
        sections.extend(reference_sections());

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
        let filter_width =
            UnicodeWidthStr::width("Filter: ") + UnicodeWidthStr::width(self.filter.as_str()) + 1;
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

fn current_section(origin: HelpOrigin) -> HelpSection {
    let rows = match origin {
        HelpOrigin::Normal {
            history_mode: true, ..
        } => rows_from_binding_refs(&[
            &HISTORY_KEYS[idx::history::NAV],
            &HISTORY_KEYS[idx::history::EXIT],
        ]),
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Result,
            result_active: true,
            ..
        } => rows_from_binding_refs(&[
            &RESULT_ACTIVE_KEYS[idx::result_active::YANK],
            &RESULT_ACTIVE_KEYS[idx::result_active::ROW_YANK],
            &RESULT_ACTIVE_KEYS[idx::result_active::STAGE_DELETE],
            &RESULT_ACTIVE_KEYS[idx::result_active::EDIT],
            &RESULT_ACTIVE_KEYS[idx::result_active::ESC_BACK],
        ]),
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Result,
            ..
        } => rows_from_binding_refs(&[
            &RESULT_ACTIVE_KEYS[idx::result_active::ENTER_DEEPEN],
            &FOOTER_NAV_KEYS[idx::footer_nav::PAGE_NAV],
            &GLOBAL_KEYS[idx::global::CSV_EXPORT],
        ]),
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Inspector,
            ..
        } => rows_from_binding_refs(&[
            &GLOBAL_KEYS[idx::global::INSPECTOR_TABS],
            &INSPECTOR_DDL_KEYS[idx::inspector_ddl::YANK],
        ]),
        HelpOrigin::Normal {
            focused_pane: FocusedPane::Explorer,
            ..
        } => rows_from_binding_refs(&[
            &GLOBAL_KEYS[idx::global::TABLE_PICKER],
            &GLOBAL_KEYS[idx::global::CONNECTIONS],
            &GLOBAL_KEYS[idx::global::SQL],
        ]),
        HelpOrigin::CommandLine => rows_from_bindings(COMMAND_LINE_KEYS),
        HelpOrigin::CellEdit => rows_from_bindings(CELL_EDIT_KEYS),
        HelpOrigin::TablePicker => rows_from_mode_rows(TABLE_PICKER_ROWS),
        HelpOrigin::CommandPalette => rows_from_mode_rows(COMMAND_PALETTE_ROWS),
        HelpOrigin::Settings => rows_from_mode_rows(SETTINGS_ROWS),
        HelpOrigin::Help => rows_from_mode_rows(HELP_ROWS),
        HelpOrigin::SqlModal { mode } => sql_current_rows(mode),
        HelpOrigin::ConnectionSetup => rows_from_bindings(CONNECTION_SETUP_KEYS),
        HelpOrigin::ConnectionError => rows_from_mode_rows(CONNECTION_ERROR_ROWS),
        HelpOrigin::ConfirmDialog => rows_from_bindings(CONFIRM_DIALOG_KEYS),
        HelpOrigin::ConnectionSelector => rows_from_mode_rows(CONNECTION_SELECTOR_ROWS),
        HelpOrigin::ErTablePicker => rows_from_mode_rows(ER_PICKER_ROWS),
        HelpOrigin::QueryHistoryPicker => rows_from_mode_rows(QUERY_HISTORY_PICKER_ROWS),
        HelpOrigin::JsonbDetail { mode } => jsonb_current_rows(mode),
        HelpOrigin::JsonbEdit => rows_from_mode_rows(JSONB_EDIT_ROWS),
    };

    HelpSection {
        title: format!("Current: {}", origin.label()),
        rows,
    }
}

fn reference_sections() -> Vec<HelpSection> {
    vec![
        section(
            "Common",
            rows_from_binding_refs(&[
                &GLOBAL_KEYS[idx::global::HELP],
                &GLOBAL_KEYS[idx::global::QUIT],
                &GLOBAL_KEYS[idx::global::SETTINGS],
                &GLOBAL_KEYS[idx::global::COMMAND_PALETTE],
                &GLOBAL_KEYS[idx::global::COMMAND_LINE],
                &GLOBAL_KEYS[idx::global::FOCUS],
                &GLOBAL_KEYS[idx::global::READ_ONLY],
            ]),
        ),
        section("Navigation", rows_from_bindings(NAVIGATION_KEYS)),
        section(
            "Open / Switch",
            rows_from_binding_refs(&[
                &GLOBAL_KEYS[idx::global::TABLE_PICKER],
                &GLOBAL_KEYS[idx::global::SQL],
                &GLOBAL_KEYS[idx::global::ER_DIAGRAM],
                &GLOBAL_KEYS[idx::global::CONNECTIONS],
                &GLOBAL_KEYS[idx::global::QUERY_HISTORY],
                &GLOBAL_KEYS[idx::global::PANE_SWITCH],
                &GLOBAL_KEYS[idx::global::INSPECTOR_TABS],
            ]),
        ),
        section(
            "Data Actions",
            merge_rows(&[
                rows_from_binding_refs(&[
                    &GLOBAL_KEYS[idx::global::RELOAD],
                    &GLOBAL_KEYS[idx::global::CSV_EXPORT],
                    &RESULT_ACTIVE_KEYS[idx::result_active::YANK],
                    &RESULT_ACTIVE_KEYS[idx::result_active::ROW_YANK],
                    &RESULT_ACTIVE_KEYS[idx::result_active::STAGE_DELETE],
                    &RESULT_ACTIVE_KEYS[idx::result_active::UNSTAGE_DELETE],
                    &INSPECTOR_DDL_KEYS[idx::inspector_ddl::YANK],
                ]),
                rows_from_mode_row_refs(&[&JSONB_DETAIL_ROWS[idx::jsonb_detail::YANK]]),
            ]),
        ),
        section(
            "Editing",
            merge_rows(&[
                rows_from_bindings(SQL_MODAL_NORMAL_KEYS),
                rows_from_bindings(SQL_MODAL_KEYS),
                rows_from_bindings(CELL_EDIT_KEYS),
                rows_from_mode_rows(JSONB_EDIT_ROWS),
                rows_from_bindings(SQL_MODAL_CONFIRMING_KEYS),
            ]),
        ),
        section(
            "Search / Filter",
            merge_rows(&[
                rows_from_mode_row_refs(&[
                    &TABLE_PICKER_ROWS[idx::table_picker::TYPE_FILTER],
                    &ER_PICKER_ROWS[idx::er_picker::TYPE_FILTER],
                    &QUERY_HISTORY_PICKER_ROWS[idx::qh_picker::TYPE_FILTER],
                ]),
                rows_from_bindings(JSONB_SEARCH_KEYS),
                rows_from_mode_rows(HELP_ROWS),
            ]),
        ),
        section(
            "Connections",
            merge_rows(&[
                rows_from_bindings(CONNECTION_SETUP_KEYS),
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
        section(
            "Advanced",
            merge_rows(&[
                rows_from_bindings(SQL_MODAL_PLAN_KEYS),
                rows_from_bindings(SQL_MODAL_COMPARE_KEYS),
                rows_from_mode_rows(ER_PICKER_ROWS),
                rows_from_mode_rows(JSONB_DETAIL_ROWS),
            ]),
        ),
    ]
}

fn sql_current_rows(mode: SqlHelpMode) -> Vec<HelpRow> {
    match mode {
        SqlHelpMode::Normal => rows_from_bindings(SQL_MODAL_NORMAL_KEYS),
        SqlHelpMode::Insert | SqlHelpMode::Running => rows_from_bindings(SQL_MODAL_KEYS),
        SqlHelpMode::Plan => rows_from_bindings(SQL_MODAL_PLAN_KEYS),
        SqlHelpMode::Compare => rows_from_bindings(SQL_MODAL_COMPARE_KEYS),
        SqlHelpMode::Confirm => rows_from_bindings(SQL_MODAL_CONFIRMING_KEYS),
    }
}

fn jsonb_current_rows(mode: JsonbHelpMode) -> Vec<HelpRow> {
    match mode {
        JsonbHelpMode::Detail => rows_from_mode_rows(JSONB_DETAIL_ROWS),
        JsonbHelpMode::Search => rows_from_bindings(JSONB_SEARCH_KEYS),
        JsonbHelpMode::Edit => rows_from_mode_rows(JSONB_EDIT_ROWS),
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

fn rows_from_binding_refs(bindings: &[&KeyBinding]) -> Vec<HelpRow> {
    rows_from_binding_iter(bindings.iter().copied())
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
    rows.iter()
        .map(|row| HelpRow::new(row.key, row.description))
        .collect()
}

fn rows_from_mode_row_refs(rows: &[&ModeRow]) -> Vec<HelpRow> {
    rows.iter()
        .map(|row| HelpRow::new(row.key, row.description))
        .collect()
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
    use crate::model::shared::input_mode::InputMode;
    use crate::model::sql_editor::modal::SqlModalTab;

    #[test]
    fn document_starts_with_current_section() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: true,
                history_mode: false,
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
    fn filter_matches_key_description_and_section_title() {
        let copy = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Result,
                result_active: true,
                history_mode: false,
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
                history_mode: false,
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
                history_mode: false,
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
    fn line_count_uses_rendered_section_metrics() {
        let document = HelpDocument::new(
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Explorer,
                result_active: false,
                history_mode: false,
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
        sql_state.modal.set_mode(InputMode::SqlModal);
        sql_state.sql_modal.set_active_tab(SqlModalTab::Plan);
        let sql_document = HelpDocument::new(HelpOrigin::from_state(&sql_state), "");

        assert_eq!(
            sql_document.sections()[0].title(),
            "Current: SQL Editor Plan"
        );

        let mut jsonb_state = AppState::new("test".to_string());
        jsonb_state.modal.set_mode(InputMode::JsonbDetail);
        jsonb_state.jsonb_detail.search_mut().active = true;
        let jsonb_document = HelpDocument::new(HelpOrigin::from_state(&jsonb_state), "");

        assert_eq!(
            jsonb_document.sections()[0].title(),
            "Current: JSONB Search"
        );
    }
}
