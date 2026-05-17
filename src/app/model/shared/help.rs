use crate::model::app_state::AppState;
use crate::model::browse::jsonb_detail::JsonbDetailMode;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::text_input::TextInputState;
use crate::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpState {
    origin: HelpOrigin,
    filter: TextInputState,
    scroll_offset: usize,
    horizontal_offset: usize,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            origin: HelpOrigin::Normal {
                focused_pane: FocusedPane::default(),
                result_active: false,
                history_mode: false,
            },
            filter: TextInputState::default(),
            scroll_offset: 0,
            horizontal_offset: 0,
        }
    }
}

impl HelpState {
    pub fn open(&mut self, origin: HelpOrigin) {
        self.origin = origin;
        self.filter.clear();
        self.reset_offsets();
    }

    pub fn close(&mut self) {
        self.filter.clear();
        self.reset_offsets();
    }

    pub fn origin(&self) -> HelpOrigin {
        self.origin
    }

    pub fn filter(&self) -> &TextInputState {
        &self.filter
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    pub fn set_horizontal_offset(&mut self, offset: usize) {
        self.horizontal_offset = offset;
    }

    pub fn reset_offsets(&mut self) {
        self.scroll_offset = 0;
        self.horizontal_offset = 0;
    }

    pub fn insert_filter_char(&mut self, ch: char) {
        self.filter.insert_char(ch);
        self.reset_offsets();
    }

    pub fn backspace_filter(&mut self) {
        self.filter.backspace();
        self.reset_offsets();
    }

    pub fn clamp_offsets(&mut self, max_scroll: usize, max_horizontal_scroll: usize) {
        self.scroll_offset = self.scroll_offset.min(max_scroll);
        self.horizontal_offset = self.horizontal_offset.min(max_horizontal_scroll);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpOrigin {
    Normal {
        focused_pane: FocusedPane,
        result_active: bool,
        history_mode: bool,
    },
    CommandLine,
    CellEdit,
    TablePicker,
    CommandPalette,
    Settings,
    Help,
    SqlModal {
        mode: SqlHelpMode,
    },
    ConnectionSetup,
    ConnectionError,
    ConfirmDialog,
    ConnectionSelector,
    ErTablePicker,
    QueryHistoryPicker,
    JsonbDetail {
        mode: JsonbHelpMode,
    },
    JsonbEdit,
}

impl HelpOrigin {
    pub fn from_state(state: &AppState) -> Self {
        match state.input_mode() {
            InputMode::Normal => Self::Normal {
                focused_pane: state.ui.focused_pane,
                result_active: state.result_interaction.selection().cell().is_some(),
                history_mode: state.query.is_history_mode(),
            },
            InputMode::CommandLine => Self::CommandLine,
            InputMode::CellEdit => Self::CellEdit,
            InputMode::TablePicker => Self::TablePicker,
            InputMode::CommandPalette => Self::CommandPalette,
            InputMode::Settings => Self::Settings,
            InputMode::Help => Self::Help,
            InputMode::SqlModal => Self::SqlModal {
                mode: SqlHelpMode::from_state(state),
            },
            InputMode::ConnectionSetup => Self::ConnectionSetup,
            InputMode::ConnectionError => Self::ConnectionError,
            InputMode::ConfirmDialog => Self::ConfirmDialog,
            InputMode::ConnectionSelector => Self::ConnectionSelector,
            InputMode::ErTablePicker => Self::ErTablePicker,
            InputMode::QueryHistoryPicker => Self::QueryHistoryPicker,
            InputMode::JsonbDetail => Self::JsonbDetail {
                mode: JsonbHelpMode::from_state(state),
            },
            InputMode::JsonbEdit => Self::JsonbEdit,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Normal {
                history_mode: true, ..
            } => "Result History",
            Self::Normal {
                focused_pane: FocusedPane::Explorer,
                ..
            } => "Explorer Pane",
            Self::Normal {
                focused_pane: FocusedPane::Inspector,
                ..
            } => "Inspector Pane",
            Self::Normal {
                focused_pane: FocusedPane::Result,
                ..
            } => "Result Pane",
            Self::CommandLine => "Command Line",
            Self::CellEdit => "Cell Edit",
            Self::TablePicker => "Table Picker",
            Self::CommandPalette => "Command Palette",
            Self::Settings => "Settings",
            Self::Help => "Help",
            Self::SqlModal { mode } => mode.label(),
            Self::ConnectionSetup => "Connection Setup",
            Self::ConnectionError => "Connection Error",
            Self::ConfirmDialog => "Confirm Dialog",
            Self::ConnectionSelector => "Connection Selector",
            Self::ErTablePicker => "ER Table Picker",
            Self::QueryHistoryPicker => "Query History Picker",
            Self::JsonbDetail { mode } => mode.label(),
            Self::JsonbEdit => "JSONB Edit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlHelpMode {
    Normal,
    Insert,
    Plan,
    Compare,
    Confirm,
    Running,
}

impl SqlHelpMode {
    fn from_state(state: &AppState) -> Self {
        match state.sql_modal.status() {
            SqlModalStatus::Editing => Self::Insert,
            SqlModalStatus::ConfirmingHigh { .. }
            | SqlModalStatus::ConfirmingAnalyzeHigh { .. } => Self::Confirm,
            SqlModalStatus::Running => Self::Running,
            SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error => {
                match state.sql_modal.active_tab() {
                    SqlModalTab::Sql => Self::Normal,
                    SqlModalTab::Plan => Self::Plan,
                    SqlModalTab::Compare => Self::Compare,
                }
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Normal => "SQL Editor",
            Self::Insert => "SQL Editor Insert",
            Self::Plan => "SQL Editor Plan",
            Self::Compare => "SQL Editor Compare",
            Self::Confirm => "SQL Editor Confirm",
            Self::Running => "SQL Editor Running",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonbHelpMode {
    Detail,
    Search,
    Edit,
}

impl JsonbHelpMode {
    fn from_state(state: &AppState) -> Self {
        if state.jsonb_detail.search().active {
            Self::Search
        } else {
            match state.jsonb_detail.mode() {
                JsonbDetailMode::Viewing => Self::Detail,
                JsonbDetailMode::Editing => Self::Edit,
                JsonbDetailMode::Searching => Self::Search,
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Detail => "JSONB Detail",
            Self::Search => "JSONB Search",
            Self::Edit => "JSONB Edit",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_origin_captures_focused_pane() {
        let mut state = AppState::new("test".to_string());
        state.ui.focused_pane = FocusedPane::Inspector;

        let origin = HelpOrigin::from_state(&state);

        assert!(matches!(
            origin,
            HelpOrigin::Normal {
                focused_pane: FocusedPane::Inspector,
                result_active: false,
                history_mode: false,
            }
        ));
    }

    #[test]
    fn normal_origin_captures_active_result_cell() {
        let mut state = AppState::new("test".to_string());
        state.result_interaction.activate_cell(1, 2);

        let origin = HelpOrigin::from_state(&state);

        assert!(matches!(
            origin,
            HelpOrigin::Normal {
                result_active: true,
                history_mode: false,
                ..
            }
        ));
    }

    #[test]
    fn normal_origin_captures_history_mode() {
        let mut state = AppState::new("test".to_string());
        state.query.enter_history(0);

        let origin = HelpOrigin::from_state(&state);

        assert!(matches!(
            origin,
            HelpOrigin::Normal {
                result_active: false,
                history_mode: true,
                ..
            }
        ));
    }

    #[test]
    fn filter_input_resets_offsets() {
        let mut state = HelpState::default();
        state.set_scroll_offset(10);
        state.set_horizontal_offset(4);

        state.insert_filter_char('c');

        assert_eq!(state.filter().content(), "c");
        assert_eq!(state.scroll_offset(), 0);
        assert_eq!(state.horizontal_offset(), 0);
    }
}
