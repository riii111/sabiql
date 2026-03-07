use super::KeyBinding;
use super::types::{Key, KeyCombo};
use crate::app::action::Action;

// =============================================================================
// SQL Modal
// =============================================================================

pub const SQL_MODAL_KEYS: &[KeyBinding] = &[
    // idx 0: SQL_RUN
    KeyBinding {
        key_short: "⌥Enter",
        key: "Alt+Enter",
        desc_short: "Run",
        description: "Execute query",
        action: Action::SqlModalSubmit,
        combos: &[KeyCombo::alt(Key::Enter)],
    },
    // idx 1: ESC_CLOSE
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseSqlModal,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 2: SQL_MOVE
    KeyBinding {
        key_short: "↑↓←→",
        key: "↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[],
    },
    // idx 3: HOME_END
    KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Line",
        description: "Line start/end",
        action: Action::None,
        combos: &[],
    },
    // idx 4: TAB
    KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Tab/Complete",
        description: "Insert tab / Accept completion",
        action: Action::None,
        combos: &[],
    },
    // idx 5: COMPLETION_TRIGGER
    KeyBinding {
        key_short: "^Space",
        key: "Ctrl+Space",
        desc_short: "Complete",
        description: "Trigger completion",
        action: Action::CompletionTrigger,
        combos: &[KeyCombo::ctrl(Key::Char(' '))],
    },
    // idx 6: CLEAR
    KeyBinding {
        key_short: "^L",
        key: "Ctrl+L",
        desc_short: "Clear",
        description: "Clear editor",
        action: Action::SqlModalClear,
        combos: &[KeyCombo::ctrl(Key::Char('l'))],
    },
];

// Keys active only while SqlModalStatus::Confirming — mutually exclusive with SQL_MODAL_KEYS.
pub const SQL_MODAL_CONFIRMING_KEYS: &[KeyBinding] = &[
    // idx 0: CONFIRM_EXECUTE
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Confirm and execute",
        action: Action::SqlModalConfirmExecute,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: CANCEL_CONFIRM
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
    // idx 0
    KeyBinding {
        key_short: ":quit",
        key: ":quit",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
        combos: &[], // command-line commands, not key combos
    },
    // idx 1
    KeyBinding {
        key_short: ":help",
        key: ":help",
        desc_short: "Help",
        description: "Show help",
        action: Action::OpenHelp,
        combos: &[],
    },
    // idx 2
    KeyBinding {
        key_short: ":sql",
        key: ":sql",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
        combos: &[],
    },
    // idx 3
    KeyBinding {
        key_short: ":erd",
        key: ":erd",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::OpenErTablePicker,
        combos: &[],
    },
    // idx 4: SUBMIT (executable, not displayed)
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Submit",
        description: "Submit command",
        action: Action::CommandLineSubmit,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 5: EXIT (executable, not displayed)
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
    // idx 0: WRITE
    KeyBinding {
        key_short: ":w",
        key: ":w",
        desc_short: "Write",
        description: "Preview and confirm UPDATE",
        action: Action::SubmitCellEditWrite,
        combos: &[], // :w is a command sequence, not a single combo
    },
    // idx 1: TYPE (display-only)
    KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Edit",
        description: "Edit cell value",
        action: Action::None,
        combos: &[],
    },
    // idx 2: MOVE (display-only)
    KeyBinding {
        key_short: "←→",
        key: "←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[],
    },
    // idx 3: HOME_END (display-only)
    KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Jump",
        description: "Jump to start/end",
        action: Action::None,
        combos: &[],
    },
    // idx 4: COMMAND
    KeyBinding {
        key_short: ":",
        key: ":",
        desc_short: "Cmd",
        description: "Open command line",
        action: Action::EnterCommandLine,
        combos: &[KeyCombo::plain(Key::Char(':'))],
    },
    // idx 5: ESC_CANCEL
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Exit to Cell Active (draft preserved)",
        action: Action::ResultCancelCellEdit,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
];
