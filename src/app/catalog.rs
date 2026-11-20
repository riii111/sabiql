use unicode_width::UnicodeWidthStr;

use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::policy::FeaturePolicy;
use crate::update::input::keymap::{Hint, HintSurface, KeyContext, current_hints, reference_hints};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpDocument {
    filter: String,
    filter_cursor: usize,
    sections: Vec<HelpSection>,
}

const FILTER_LINE_COUNT: usize = 1;
const FILTER_SECTION_GAP_LINES: usize = 1;
const SECTION_HEADER_LINE_COUNT: usize = 1;
pub const HELP_KEY_INDENT_WIDTH: usize = 2;
pub const HELP_KEY_DESC_GAP: usize = 2;

impl HelpDocument {
    pub fn from_state(state: &AppState) -> Self {
        let filter = state.ui.help().filter();
        let context = state.ui.help().origin().cloned().unwrap_or_else(|| {
            KeyContext::for_scope(
                InputMode::Normal,
                state.settings.saved_keymap_preset(),
                &FeaturePolicy::new(&state.session.active_engine_feature_profile()),
            )
        });
        Self::new(context, filter.content(), filter.cursor())
    }

    fn new(context: KeyContext, filter: &str, filter_cursor: usize) -> Self {
        let normalized = filter.trim().to_lowercase();
        let mut sections = vec![current_section(&context)];
        sections.extend(reference_sections(&context));

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

fn current_section(context: &KeyContext) -> HelpSection {
    HelpSection {
        title: format!("Current: {}", context.label()),
        rows: current_hints(context)
            .into_iter()
            .map(|hint| HelpRow::new(hint.key, hint.description))
            .collect(),
    }
}

fn reference_sections(context: &KeyContext) -> Vec<HelpSection> {
    [
        ("Common", HintSurface::Common),
        ("Navigation", HintSurface::Navigation),
        ("Open / Switch", HintSurface::OpenSwitch),
        ("Data Actions", HintSurface::DataActions),
        ("Editing", HintSurface::Editing),
        ("Search / Filter", HintSurface::SearchFilter),
        ("Connections", HintSurface::Connections),
        ("Modal Basics", HintSurface::ModalBasics),
        ("Advanced", HintSurface::Advanced),
    ]
    .into_iter()
    .map(|(title, surface)| section(title, rows_from_hints(reference_hints(surface, context))))
    .filter(|section| !section.rows.is_empty())
    .collect()
}

fn section(title: &str, rows: Vec<HelpRow>) -> HelpSection {
    HelpSection {
        title: title.to_string(),
        rows: merge_rows(&[rows]),
    }
}

fn rows_from_hints(hints: Vec<Hint>) -> Vec<HelpRow> {
    hints
        .into_iter()
        .map(|hint| HelpRow::new(hint.key, hint.description))
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
    use crate::domain::{
        ColumnAttributes, ConnectionId, DatabaseType, QueryResult, QuerySource, QueryValue,
    };
    use crate::model::connection::setup::ConnectionField;
    use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
    use crate::model::shared::focused_pane::FocusedPane;
    use crate::model::shared::settings::KeymapPreset;
    use crate::model::sql_editor::modal::SqlModalTab;
    use crate::policy::FeatureRequirement;
    use crate::test_support;
    use std::sync::Arc;

    fn row_descriptions(document: &HelpDocument) -> Vec<&str> {
        document
            .sections()
            .iter()
            .flat_map(HelpSection::rows)
            .map(HelpRow::description)
            .collect()
    }

    #[test]
    fn normal_help_finds_settings_and_command_operations() {
        let mut state = AppState::new("test".to_string());
        let context = KeyContext::from_state(&state);
        state.ui.help_mut().open(context);
        let document = HelpDocument::from_state(&state);
        for description in [
            "Select setting",
            "Switch settings section",
            "Edit custom browser",
            "Submit command",
        ] {
            assert!(
                document
                    .sections()
                    .iter()
                    .skip(1)
                    .flat_map(HelpSection::rows)
                    .any(|row| row.description() == description),
                "{description}"
            );
            let filtered = HelpDocument::new(
                KeyContext::from_state(&state),
                description,
                description.len(),
            );
            assert!(row_descriptions(&filtered).contains(&description));
        }
    }

    #[test]
    fn normal_help_references_commands_and_conditional_navigation() {
        for preset in [KeymapPreset::Default, KeymapPreset::Ide] {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "database",
                DatabaseType::PostgreSQL,
                "postgres://test",
            );
            state.settings.load_keymap_preset(preset);
            let context = KeyContext::from_state(&state);
            state.ui.help_mut().open(context.clone());
            let document = HelpDocument::from_state(&state);
            let rows: Vec<_> = document
                .sections()
                .iter()
                .skip(1)
                .flat_map(HelpSection::rows)
                .collect();
            for key in [
                ":quit",
                ":help",
                ":sql",
                ":erd",
                ":settings",
                ":palette",
                ":w",
            ] {
                assert!(rows.iter().any(|row| row.key() == key), "{preset:?}: {key}");
                let filtered = HelpDocument::new(context.clone(), key, key.len());
                assert!(
                    filtered
                        .sections()
                        .iter()
                        .flat_map(HelpSection::rows)
                        .any(|row| row.key() == key),
                    "filtered {key}"
                );
            }
            for description in [
                "Exit command line",
                "Enter cell selection at the current viewport anchor",
                "Exit cell selection",
                "Discard pending draft",
                "Exit Focus mode",
                "Disable Read-Only mode",
            ] {
                assert!(
                    rows.iter().any(|row| row.description() == description),
                    "{description}"
                );
            }
            assert!(rows.iter().any(|row| row.description() == "Navigate"
                && row.key().contains('j')
                && row.key().contains('k')));
        }

        let mut sqlite = AppState::new("test".to_string());
        sqlite.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "sqlite",
            DatabaseType::SQLite,
            "sqlite:///tmp/test.db",
        );
        let document = HelpDocument::new(KeyContext::from_state(&sqlite), "", 0);
        assert!(
            !document
                .sections()
                .iter()
                .skip(1)
                .flat_map(HelpSection::rows)
                .any(|row| row.key() == ":erd")
        );
    }

    #[test]
    fn reload_is_current_only_when_connected_but_always_in_reference() {
        let mut state = AppState::new("test".to_string());
        let context = KeyContext::from_state(&state);
        state.ui.help_mut().open(context);
        let document = HelpDocument::from_state(&state);
        assert!(
            !document.sections()[0]
                .rows()
                .iter()
                .any(|row| row.description() == "Reload metadata")
        );
        assert!(
            document
                .sections()
                .iter()
                .find(|section| section.title() == "Data Actions")
                .unwrap()
                .rows()
                .iter()
                .any(|row| row.description() == "Reload metadata")
        );

        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://test",
        );
        let document = HelpDocument::new(KeyContext::from_state(&state), "", 0);
        assert!(
            document.sections()[0]
                .rows()
                .iter()
                .any(|row| row.description() == "Reload metadata")
        );
    }

    #[test]
    fn pending_sequence_does_not_filter_reference_sections() {
        use crate::model::shared::key_sequence::{KeySequenceState, Prefix};

        let mut state = AppState::new("test".to_string());
        let idle = HelpDocument::new(KeyContext::from_state(&state), "", 0);
        state
            .ui
            .set_key_sequence(KeySequenceState::WaitingSecondKey(Prefix::G));
        let pending = HelpDocument::new(KeyContext::from_state(&state), "", 0);
        assert_eq!(&idle.sections()[1..], &pending.sections()[1..]);
    }

    #[test]
    fn help_origins_exclude_common_and_navigation_from_current() {
        use crate::ports::inbound::{InputEvent, Key, KeyCombo};
        use crate::services::AppServices;
        use crate::update::input::handle_event;
        use crate::update::reducer::reduce;
        use std::time::Instant;

        fn press(state: &mut AppState, key: Key) {
            let action = handle_event(InputEvent::Key(KeyCombo::plain(key)), state);
            reduce(state, action, Instant::now(), &AppServices::stub());
        }
        fn has_row(document: &HelpDocument, section: &str, key: &str, description: &str) -> bool {
            document
                .sections()
                .iter()
                .find(|group| group.title().starts_with(section))
                .is_some_and(|group| {
                    group
                        .rows()
                        .iter()
                        .any(|row| row.key() == key && row.description() == description)
                })
        }

        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://test",
        );
        press(&mut state, Key::Char('?'));
        let document = HelpDocument::from_state(&state);
        for (key, description) in [("Enter", "Select table"), ("Esc", "Cancel")] {
            assert!(!has_row(&document, "Current:", key, description));
            assert!(has_row(&document, "Navigation", key, description));
        }

        press(&mut state, Key::Esc);
        press(&mut state, Key::Char(':'));
        for ch in "help".chars() {
            press(&mut state, Key::Char(ch));
        }
        press(&mut state, Key::Enter);
        assert_eq!(state.input_mode(), InputMode::Help);
        let document = HelpDocument::from_state(&state);
        assert_eq!(document.sections()[0].title(), "Current: Command Line");
        for (key, description) in [(":quit", "Quit application"), (":help", "Show help")] {
            assert!(!has_row(&document, "Current:", key, description));
            assert!(has_row(&document, "Common", key, description));
        }

        press(&mut state, Key::Esc);
        state.ui.set_focused_pane(FocusedPane::Result);
        state.result_interaction.activate_cell(0, 0);
        state
            .result_interaction
            .begin_cell_edit(0, 0, "before".to_string());
        state
            .result_interaction
            .replace_cell_edit_draft("after".to_string());
        press(&mut state, Key::Char('?'));
        let document = HelpDocument::from_state(&state);
        assert!(has_row(
            &document,
            "Current:",
            "Esc",
            "Discard pending draft"
        ));
        assert!(has_row(
            &document,
            "Editing",
            "Esc",
            "Discard pending draft"
        ));
    }

    fn result_state_with_selected_column(
        database_type: DatabaseType,
        selected_column: usize,
        data_type: &str,
        attributes: ColumnAttributes,
        value: QueryValue,
    ) -> AppState {
        let (schema, name, dsn) = match database_type {
            DatabaseType::SQLite => ("main", "users", "sqlite://test.db"),
            DatabaseType::MySQL => ("sabiql_test", "users", "mysql://localhost/test"),
            DatabaseType::PostgreSQL => ("public", "users", "postgres://localhost/test"),
        };
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            database_type,
            dsn,
        );
        state
            .query
            .set_current_result(Arc::new(QueryResult::success_with_values(
                "SELECT id, value FROM users".to_string(),
                vec!["id".to_string(), "value".to_string()],
                vec![vec![QueryValue::text("1"), value]],
                1,
                QuerySource::Preview,
            )));
        state.query.pagination.reset_for_table(schema, name);

        let mut table = test_support::table::minimal(schema, name);
        let mut id = test_support::column::test_nullable_column("id", "INTEGER", 1);
        id.attributes = if selected_column == 0 {
            attributes
        } else {
            ColumnAttributes::PRIMARY_KEY
        };
        let mut selected = test_support::column::test_nullable_column("value", data_type, 2);
        selected.attributes = if selected_column == 1 {
            attributes
        } else {
            ColumnAttributes::empty()
        };
        table.columns = vec![id, selected];
        table.primary_key = Some(vec!["id".to_string()]);
        state.session.set_table_detail_raw(Some(table));
        state.ui.set_focused_pane(FocusedPane::Result);
        state.result_interaction.activate_cell(0, selected_column);
        state
    }

    fn current_descriptions(state: &mut AppState) -> Vec<String> {
        let origin = KeyContext::from_state(state);
        state.ui.help_mut().open(origin);
        HelpDocument::from_state(state).sections()[0]
            .rows()
            .iter()
            .map(|row| row.description().to_string())
            .collect()
    }

    #[test]
    fn result_current_help_shows_edit_only_for_editable_selected_cells() {
        let cases = vec![
            (
                "ordinary text",
                result_state_with_selected_column(
                    DatabaseType::PostgreSQL,
                    1,
                    "text",
                    ColumnAttributes::empty(),
                    QueryValue::text("before"),
                ),
                true,
            ),
            (
                "JSON cell",
                result_state_with_selected_column(
                    DatabaseType::PostgreSQL,
                    1,
                    "jsonb",
                    ColumnAttributes::empty(),
                    QueryValue::text(r#"{"key":"value"}"#),
                ),
                true,
            ),
            (
                "primary key",
                result_state_with_selected_column(
                    DatabaseType::PostgreSQL,
                    0,
                    "integer",
                    ColumnAttributes::PRIMARY_KEY,
                    QueryValue::text("1"),
                ),
                false,
            ),
            (
                "read-only column",
                result_state_with_selected_column(
                    DatabaseType::PostgreSQL,
                    1,
                    "integer",
                    ColumnAttributes::READ_ONLY,
                    QueryValue::text("1"),
                ),
                false,
            ),
            (
                "SQLite BLOB",
                result_state_with_selected_column(
                    DatabaseType::SQLite,
                    1,
                    "BLOB",
                    ColumnAttributes::empty(),
                    QueryValue::Blob(vec![0, 255]),
                ),
                false,
            ),
        ];

        for (case_name, mut state, expected_edit_hint) in cases {
            assert!(
                state.can_write_visible_preview(),
                "fixture must keep table writes enabled for {case_name}"
            );
            let descriptions = current_descriptions(&mut state);

            assert_eq!(
                descriptions
                    .iter()
                    .any(|description| description == "Edit active cell"),
                expected_edit_hint,
                "unexpected edit hint for {case_name}"
            );
        }
    }

    #[test]
    fn result_current_help_uses_the_draft_snapshot_for_escape() {
        let mut state = result_state_with_selected_column(
            DatabaseType::PostgreSQL,
            1,
            "text",
            ColumnAttributes::empty(),
            QueryValue::text("before"),
        );
        state
            .result_interaction
            .begin_cell_edit(0, 1, "before".to_string());
        state
            .result_interaction
            .replace_cell_edit_draft("after".to_string());
        state.modal.set_mode(InputMode::Normal);

        let descriptions = current_descriptions(&mut state);

        assert!(
            descriptions
                .iter()
                .any(|description| description == "Discard pending draft")
        );
        assert!(
            !descriptions
                .iter()
                .any(|description| { description == "Exit cell selection" })
        );

        state.result_interaction.discard_cell_edit();
        state.result_interaction.activate_cell(0, 0);
        let document = HelpDocument::from_state(&state);
        assert!(
            document.sections()[0]
                .rows()
                .iter()
                .any(|row| { row.description() == "Discard pending draft" })
        );
    }

    #[test]
    fn origin_from_state_maps_sql_plan_and_json_search() {
        let mut sql_state = AppState::new("test".to_string());
        sql_state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://localhost/test",
        );
        sql_state.modal.set_mode(InputMode::SqlModal);
        sql_state.sql_modal.set_active_tab(SqlModalTab::Plan);
        let sql_document = HelpDocument::new(KeyContext::from_state(&sql_state), "", 0);

        assert_eq!(
            sql_document.sections()[0].title(),
            "Current: SQL Editor Plan"
        );

        let mut json_state = AppState::new("test".to_string());
        json_state.modal.set_mode(InputMode::JsonDetail);
        json_state.json_detail.enter_search();
        let json_document = HelpDocument::new(KeyContext::from_state(&json_state), "", 0);

        assert_eq!(json_document.sections()[0].title(), "Current: JSON Search");
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
        let origin = KeyContext::from_state(&state);
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
    fn json_help_rows_follow_feature_policy() {
        for database_type in [
            DatabaseType::PostgreSQL,
            DatabaseType::MySQL,
            DatabaseType::SQLite,
        ] {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "database",
                database_type,
                "database",
            );
            let origin = KeyContext::from_state(&state);
            state.ui.help_mut().open(origin);

            let document = HelpDocument::from_state(&state);
            let descriptions = row_descriptions(&document);
            let help_includes_json = descriptions
                .iter()
                .any(|description| description.contains("Close JSON detail"));
            let profile = EngineFeatureProfile::for_database_type(database_type);
            let feature_policy = FeaturePolicy::new(&profile);
            let policy_allows_json =
                feature_policy.is_enabled(FeatureRequirement::JsonDocumentDetail);

            assert_eq!(help_includes_json, policy_allows_json);
            assert!(
                descriptions
                    .iter()
                    .all(|description| !description.contains("JSONB"))
            );
        }
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

        let document = HelpDocument::new(KeyContext::from_state(&state), "", 0);

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
        let origin = KeyContext::from_state(&state);
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
    fn reference_sections_and_filter_use_derived_rows() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "postgres://test",
        );
        let context = KeyContext::from_state(&state);
        let document = HelpDocument::new(context.clone(), "", 0);
        let titles: Vec<_> = document.sections().iter().map(HelpSection::title).collect();
        for title in [
            "Common",
            "Navigation",
            "Open / Switch",
            "Data Actions",
            "Editing",
            "Search / Filter",
            "Connections",
            "Modal Basics",
            "Advanced",
        ] {
            assert!(titles.contains(&title), "missing {title}");
        }
        let editing = document
            .sections()
            .iter()
            .find(|section| section.title() == "Editing")
            .unwrap();
        assert!(
            editing
                .rows()
                .iter()
                .any(|row| row.key() == "Ctrl+A" && row.description() == "Move to line start")
        );
        let filtered = HelpDocument::new(context.clone(), "line start", 4);
        assert_eq!(filtered.filter_cursor(), 4);
        assert!(
            filtered
                .sections()
                .iter()
                .flat_map(HelpSection::rows)
                .any(|row| row.description() == "Move to line start")
        );
        let no_matches = HelpDocument::new(context, "not-a-real-command", 100);
        assert_eq!(no_matches.sections()[0].title(), "No matches");
        assert_eq!(no_matches.filter_cursor(), "not-a-real-command".len());
        assert!(document.line_count() > document.sections().len());
        assert!(document.content_width() >= document.key_column_width());
    }

    #[test]
    fn connection_setup_help_keeps_the_open_dropdown_snapshot() {
        let mut state = AppState::new("test".to_string());
        state.settings.load_keymap_preset(KeymapPreset::Ide);
        state.modal.set_mode(InputMode::ConnectionSetup);
        state.connection_setup.toggle_focused_dropdown();
        let context = KeyContext::from_state(&state);
        state.ui.help_mut().open(context);
        state.connection_setup.toggle_focused_dropdown();
        let document = HelpDocument::from_state(&state);
        let rows = document.sections()[0].rows();
        assert!(
            rows.iter().any(
                |row| row.key() == "Enter" && row.description() == "Confirm dropdown selection"
            )
        );
        assert!(
            rows.iter()
                .any(|row| row.key() == "Ctrl+S" && row.description() == "Save and connect")
        );
    }

    #[test]
    fn sql_help_keeps_the_visible_completion_snapshot() {
        use crate::model::sql_editor::completion::{CompletionCandidate, CompletionKind};

        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.enter_editing();
        state.sql_modal.apply_completion_update(
            &[CompletionCandidate {
                text: "users".to_string(),
                kind: CompletionKind::Table,
                score: 1,
            }],
            0,
            true,
        );
        let context = KeyContext::from_state(&state);
        state.ui.help_mut().open(context);
        state.sql_modal.dismiss_completion();

        let document = HelpDocument::from_state(&state);
        let rows = document.sections()[0].rows();
        assert!(
            rows.iter()
                .any(|row| row.description() == "Accept completion" && row.key().contains("Enter"))
        );
        assert!(
            rows.iter()
                .any(|row| row.description() == "Dismiss completion" && row.key().contains("Esc"))
        );
    }

    #[test]
    fn connection_setup_cleartext_auth_is_a_dropdown() {
        let policy = FeaturePolicy::new(&EngineFeatureProfile::postgres_like());
        let context = KeyContext::for_scope(InputMode::ConnectionSetup, KeymapPreset::Ide, &policy)
            .with_connection_setup(ConnectionField::CleartextAuth, false);
        let document = HelpDocument::new(context, "", 0);
        let rows = document.sections()[0].rows();
        assert!(
            rows.iter()
                .any(|row| row.key() == "Enter" && row.description() == "Toggle dropdown")
        );
        assert!(
            rows.iter()
                .any(|row| row.key() == "Ctrl+S" && row.description() == "Save and connect")
        );
    }
}
