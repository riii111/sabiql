use super::KeyBinding;
use super::types::{Key, KeyCombo};
use crate::app::update::action::Action;

// =============================================================================
// SQL Modal (Normal mode — default when opened)
// =============================================================================

pub const SQL_MODAL_NORMAL_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "⌥Enter",
        key: "Alt+Enter",
        desc_short: "Run",
        description: "Execute query",
        action: Action::SqlModalSubmit,
        combos: &[KeyCombo::alt(Key::Enter)],
    },
    KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy query to clipboard",
        action: Action::SqlModalYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    },
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Insert",
        description: "Enter Insert mode",
        action: Action::SqlModalEnterInsert,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    KeyBinding {
        key_short: "↑↓←→",
        key: "↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Line",
        description: "Line start/end",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseSqlModal,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    KeyBinding {
        key_short: "^L",
        key: "Ctrl+L",
        desc_short: "Clear",
        description: "Clear editor",
        action: Action::SqlModalClear,
        combos: &[KeyCombo::ctrl(Key::Char('l'))],
    },
    KeyBinding {
        key_short: "^O",
        key: "Ctrl+O",
        desc_short: "History",
        description: "Open Query History",
        action: Action::OpenQueryHistoryPicker,
        combos: &[KeyCombo::ctrl(Key::Char('o'))],
    },
];

// =============================================================================
// SQL Modal — Plan tab (read-only viewer)
// =============================================================================

pub const SQL_MODAL_PLAN_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "^E",
        key: "Ctrl+E",
        desc_short: "Explain",
        description: "Run EXPLAIN on current query",
        action: Action::ExplainRequest,
        combos: &[KeyCombo::ctrl(Key::Char('e'))],
    },
    KeyBinding {
        key_short: "\u{2325}E",
        key: "Alt+E",
        desc_short: "Analyze",
        description: "Run EXPLAIN ANALYZE on current query",
        action: Action::ExplainAnalyzeRequest,
        combos: &[KeyCombo::alt(Key::Char('e'))],
    },
    KeyBinding {
        key_short: "b",
        key: "b",
        desc_short: "Pin",
        description: "Pin left slot",
        action: Action::SaveExplainBaseline,
        combos: &[KeyCombo::plain(Key::Char('b'))],
    },
    KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy to clipboard",
        action: Action::SqlModalYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    },
    KeyBinding {
        key_short: "\u{2191}\u{2193}",
        key: "↑↓/jk",
        desc_short: "Scroll",
        description: "Scroll plan text",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Switch",
        description: "Switch tab",
        action: Action::SqlModalNextTab,
        combos: &[KeyCombo::plain(Key::Tab)],
    },
    KeyBinding {
        key_short: "⇧Tab",
        key: "Shift+Tab",
        desc_short: "Prev",
        description: "Previous tab",
        action: Action::SqlModalPrevTab,
        combos: &[KeyCombo::plain(Key::BackTab)],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseSqlModal,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

// =============================================================================
// SQL Modal — Compare tab (read-only viewer)
// =============================================================================

pub const SQL_MODAL_COMPARE_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "^E",
        key: "Ctrl+E",
        desc_short: "Explain",
        description: "Run EXPLAIN on current query",
        action: Action::ExplainRequest,
        combos: &[KeyCombo::ctrl(Key::Char('e'))],
    },
    KeyBinding {
        key_short: "\u{2325}E",
        key: "Alt+E",
        desc_short: "Analyze",
        description: "Run EXPLAIN ANALYZE on current query",
        action: Action::ExplainAnalyzeRequest,
        combos: &[KeyCombo::alt(Key::Char('e'))],
    },
    KeyBinding {
        key_short: "l",
        key: "l",
        desc_short: "Left",
        description: "Cycle left comparison slot",
        action: Action::CompareSelectLeftSlot,
        combos: &[KeyCombo::plain(Key::Char('l'))],
    },
    KeyBinding {
        key_short: "r",
        key: "r",
        desc_short: "Right",
        description: "Cycle right comparison slot",
        action: Action::CompareSelectRightSlot,
        combos: &[KeyCombo::plain(Key::Char('r'))],
    },
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit query in SQL tab",
        action: Action::CompareEditQuery,
        combos: &[KeyCombo::plain(Key::Char('e'))],
    },
    KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy to clipboard",
        action: Action::SqlModalYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    },
    KeyBinding {
        key_short: "\u{2191}\u{2193}",
        key: "↑↓/jk",
        desc_short: "Scroll",
        description: "Scroll comparison text",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Switch",
        description: "Switch tab",
        action: Action::SqlModalNextTab,
        combos: &[KeyCombo::plain(Key::Tab)],
    },
    KeyBinding {
        key_short: "⇧Tab",
        key: "Shift+Tab",
        desc_short: "Prev",
        description: "Previous tab",
        action: Action::SqlModalPrevTab,
        combos: &[KeyCombo::plain(Key::BackTab)],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseSqlModal,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

// =============================================================================
// SQL Modal (Insert mode)
// =============================================================================

pub const SQL_MODAL_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "⌥Enter",
        key: "Alt+Enter",
        desc_short: "Run",
        description: "Execute query",
        action: Action::SqlModalSubmit,
        combos: &[KeyCombo::alt(Key::Enter)],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Return to Normal mode",
        action: Action::SqlModalEnterNormal,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    KeyBinding {
        key_short: "↑↓←→",
        key: "↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Line",
        description: "Line start/end",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Tab/Complete",
        description: "Insert tab / Accept completion",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "^Space",
        key: "Ctrl+Space",
        desc_short: "Complete",
        description: "Trigger completion",
        action: Action::CompletionTrigger,
        combos: &[KeyCombo::ctrl(Key::Char(' '))],
    },
    KeyBinding {
        key_short: "^L",
        key: "Ctrl+L",
        desc_short: "Clear",
        description: "Clear editor",
        action: Action::SqlModalClear,
        combos: &[KeyCombo::ctrl(Key::Char('l'))],
    },
    KeyBinding {
        key_short: "^O",
        key: "Ctrl+O",
        desc_short: "History",
        description: "Open Query History",
        action: Action::OpenQueryHistoryPicker,
        combos: &[KeyCombo::ctrl(Key::Char('o'))],
    },
];

// Keys active only while SqlModalStatus::Confirming — mutually exclusive with SQL_MODAL_KEYS.
pub const SQL_MODAL_CONFIRMING_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Confirm and execute",
        action: Action::SqlModalConfirmExecute,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Back",
        description: "Cancel and return to editor",
        action: Action::SqlModalCancelConfirm,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

// =============================================================================
// Command Line
// =============================================================================

pub const COMMAND_LINE_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: ":quit",
        key: ":quit",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
        combos: &[], // command-line commands, not key combos
    },
    KeyBinding {
        key_short: ":help",
        key: ":help",
        desc_short: "Help",
        description: "Show help",
        action: Action::OpenHelp,
        combos: &[],
    },
    KeyBinding {
        key_short: ":sql",
        key: ":sql",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
        combos: &[],
    },
    KeyBinding {
        key_short: ":erd",
        key: ":erd",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::OpenErTablePicker,
        combos: &[],
    },
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Submit",
        description: "Submit command",
        action: Action::CommandLineSubmit,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Exit",
        description: "Exit command line",
        action: Action::ExitCommandLine,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];

// =============================================================================
// Cell Edit
// =============================================================================

pub const CELL_EDIT_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: ":w",
        key: ":w",
        desc_short: "Write",
        description: "Preview and confirm UPDATE",
        action: Action::SubmitCellEditWrite,
        combos: &[], // :w is a command sequence, not a single combo
    },
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Edit",
        description: "Edit cell value",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "←→",
        key: "←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Jump",
        description: "Jump to start/end",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: ":",
        key: ":",
        desc_short: "Cmd",
        description: "Open command line",
        action: Action::EnterCommandLine,
        combos: &[KeyCombo::plain(Key::Char(':'))],
    },
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Exit to Cell Active (draft preserved)",
        action: Action::ResultCancelCellEdit,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];
