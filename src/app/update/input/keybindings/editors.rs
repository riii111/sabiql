use super::KeyBinding;
use super::{Key, KeyCombo};
use crate::update::action::{Action, ModalKind};

// =============================================================================
// SQL Modal (Normal mode — default when opened)
// =============================================================================

pub mod sql_modal_normal {
    use crate::update::action::{Action, ModalKind};
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const RUN: KeyBinding = KeyBinding {
        key_short: "⌥Enter/F5",
        key: "Alt+Enter / F5",
        desc_short: "Run",
        description: "Execute query",
        action: Action::SqlModalSubmit,
        combos: &[KeyCombo::alt(Key::Enter), KeyCombo::plain(Key::F(5))],
    };

    pub const YANK: KeyBinding = KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy query to clipboard",
        action: Action::SqlModalYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    };

    pub const ENTER_INSERT: KeyBinding = KeyBinding {
        key_short: "i",
        key: "i",
        desc_short: "Insert",
        description: "Enter Insert mode",
        action: Action::SqlModalEnterInsert,
        combos: &[KeyCombo::plain(Key::Char('i'))],
    };

    pub const APPEND: KeyBinding = KeyBinding {
        key_short: "A",
        key: "A",
        desc_short: "Append",
        description: "Append at line end",
        action: Action::SqlModalAppendInsert,
        combos: &[KeyCombo::plain(Key::Char('A'))],
    };

    pub const MOVE: KeyBinding = KeyBinding {
        key_short: "hjkl",
        key: "h / j / k / l / ↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[],
    };

    pub const HOME_END: KeyBinding = KeyBinding {
        key_short: "0$wb",
        key: "0 / $ / w / b / Home / End",
        desc_short: "Jump",
        description: "Move by word or line boundary",
        action: Action::None,
        combos: &[],
    };

    pub const VIEWPORT: KeyBinding = KeyBinding {
        key_short: "ggGHML",
        key: "gg / G / H / M / L",
        desc_short: "View",
        description: "Jump by buffer or viewport",
        action: Action::None,
        combos: &[],
    };

    pub const CLOSE: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseModal(ModalKind::SqlModal),
        combos: &[KeyCombo::plain(Key::Esc)],
    };

    pub const CLEAR: KeyBinding = KeyBinding {
        key_short: "^L",
        key: "Ctrl+L",
        desc_short: "Clear",
        description: "Clear editor",
        action: Action::SqlModalClear,
        combos: &[KeyCombo::ctrl(Key::Char('l'))],
    };

    pub const QUERY_HISTORY: KeyBinding = KeyBinding {
        key_short: "^O",
        key: "Ctrl+O",
        desc_short: "History",
        description: "Open Query History",
        action: Action::OpenModal(ModalKind::QueryHistoryPicker),
        combos: &[KeyCombo::ctrl(Key::Char('o'))],
    };
}

pub const SQL_MODAL_NORMAL_KEYS: &[KeyBinding] = &[
    sql_modal_normal::RUN,
    sql_modal_normal::YANK,
    sql_modal_normal::ENTER_INSERT,
    sql_modal_normal::APPEND,
    sql_modal_normal::MOVE,
    sql_modal_normal::HOME_END,
    sql_modal_normal::VIEWPORT,
    sql_modal_normal::CLOSE,
    sql_modal_normal::CLEAR,
    sql_modal_normal::QUERY_HISTORY,
];

// =============================================================================
// SQL Modal — Plan tab (read-only viewer)
// =============================================================================

pub mod sql_modal_plan {
    use crate::update::action::{Action, ModalKind};
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const EXPLAIN: KeyBinding = KeyBinding {
        key_short: "^E",
        key: "Ctrl+E",
        desc_short: "Explain",
        description: "Run EXPLAIN on current query",
        action: Action::ExplainRequest,
        combos: &[KeyCombo::ctrl(Key::Char('e'))],
    };

    pub const ANALYZE: KeyBinding = KeyBinding {
        key_short: "\u{2325}E",
        key: "Alt+E",
        desc_short: "Analyze",
        description: "Run EXPLAIN ANALYZE on current query",
        action: Action::ExplainAnalyzeRequest,
        combos: &[KeyCombo::alt(Key::Char('e'))],
    };

    pub const YANK: KeyBinding = KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy to clipboard",
        action: Action::SqlModalYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    };

    pub const SCROLL: KeyBinding = KeyBinding {
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑↓ / j / k",
        desc_short: "Scroll",
        description: "Scroll plan text",
        action: Action::None,
        combos: &[],
    };

    pub const TAB: KeyBinding = KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Switch",
        description: "Switch tab",
        action: Action::SqlModalNextTab,
        combos: &[KeyCombo::plain(Key::Tab)],
    };

    pub const BACKTAB: KeyBinding = KeyBinding {
        key_short: "⇧Tab",
        key: "Shift+Tab",
        desc_short: "Prev",
        description: "Previous tab",
        action: Action::SqlModalPrevTab,
        combos: &[KeyCombo::plain(Key::BackTab)],
    };

    pub const CLOSE: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseModal(ModalKind::SqlModal),
        combos: &[KeyCombo::plain(Key::Esc)],
    };
}

pub const SQL_MODAL_PLAN_KEYS: &[KeyBinding] = &[
    sql_modal_plan::EXPLAIN,
    sql_modal_plan::ANALYZE,
    sql_modal_plan::YANK,
    sql_modal_plan::SCROLL,
    sql_modal_plan::TAB,
    sql_modal_plan::BACKTAB,
    sql_modal_plan::CLOSE,
];

// =============================================================================
// SQL Modal — Compare tab (read-only viewer)
// =============================================================================

pub mod sql_modal_compare {
    use crate::update::action::{Action, ModalKind};
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const EXPLAIN: KeyBinding = KeyBinding {
        key_short: "^E",
        key: "Ctrl+E",
        desc_short: "Explain",
        description: "Run EXPLAIN on current query",
        action: Action::ExplainRequest,
        combos: &[KeyCombo::ctrl(Key::Char('e'))],
    };

    pub const ANALYZE: KeyBinding = KeyBinding {
        key_short: "\u{2325}E",
        key: "Alt+E",
        desc_short: "Analyze",
        description: "Run EXPLAIN ANALYZE on current query",
        action: Action::ExplainAnalyzeRequest,
        combos: &[KeyCombo::alt(Key::Char('e'))],
    };

    pub const EDIT_QUERY: KeyBinding = KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "Edit",
        description: "Edit query in SQL tab",
        action: Action::CompareEditQuery,
        combos: &[KeyCombo::plain(Key::Char('e'))],
    };

    pub const YANK: KeyBinding = KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy to clipboard",
        action: Action::SqlModalYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    };

    pub const SCROLL: KeyBinding = KeyBinding {
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑↓ / j / k",
        desc_short: "Scroll",
        description: "Scroll comparison text",
        action: Action::None,
        combos: &[],
    };

    pub const TAB: KeyBinding = KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Switch",
        description: "Switch tab",
        action: Action::SqlModalNextTab,
        combos: &[KeyCombo::plain(Key::Tab)],
    };

    pub const BACKTAB: KeyBinding = KeyBinding {
        key_short: "⇧Tab",
        key: "Shift+Tab",
        desc_short: "Prev",
        description: "Previous tab",
        action: Action::SqlModalPrevTab,
        combos: &[KeyCombo::plain(Key::BackTab)],
    };

    pub const CLOSE: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close editor",
        action: Action::CloseModal(ModalKind::SqlModal),
        combos: &[KeyCombo::plain(Key::Esc)],
    };
}

pub const SQL_MODAL_COMPARE_KEYS: &[KeyBinding] = &[
    sql_modal_compare::EXPLAIN,
    sql_modal_compare::ANALYZE,
    sql_modal_compare::EDIT_QUERY,
    sql_modal_compare::YANK,
    sql_modal_compare::SCROLL,
    sql_modal_compare::TAB,
    sql_modal_compare::BACKTAB,
    sql_modal_compare::CLOSE,
];

// =============================================================================
// SQL Modal (Insert mode)
// =============================================================================

pub mod sql_modal {
    use crate::update::action::{Action, ModalKind};
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const RUN: KeyBinding = KeyBinding {
        key_short: "⌥Enter/F5",
        key: "Alt+Enter / F5",
        desc_short: "Run",
        description: "Execute query",
        action: Action::SqlModalSubmit,
        combos: &[KeyCombo::alt(Key::Enter), KeyCombo::plain(Key::F(5))],
    };

    pub const ESC_NORMAL: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Return to Normal mode",
        action: Action::SqlModalEnterNormal,
        combos: &[KeyCombo::plain(Key::Esc)],
    };

    pub const MOVE: KeyBinding = KeyBinding {
        key_short: "↑↓←→",
        key: "↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[],
    };

    pub const HOME_END: KeyBinding = KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Line",
        description: "Line start/end",
        action: Action::None,
        combos: &[],
    };

    pub const TAB: KeyBinding = KeyBinding {
        key_short: "Tab",
        key: "Tab",
        desc_short: "Tab/Complete",
        description: "Insert tab / Accept completion",
        action: Action::None,
        combos: &[],
    };

    pub const CLEAR: KeyBinding = KeyBinding {
        key_short: "^L",
        key: "Ctrl+L",
        desc_short: "Clear",
        description: "Clear editor",
        action: Action::SqlModalClear,
        combos: &[KeyCombo::ctrl(Key::Char('l'))],
    };

    pub const QUERY_HISTORY: KeyBinding = KeyBinding {
        key_short: "^O",
        key: "Ctrl+O",
        desc_short: "History",
        description: "Open Query History",
        action: Action::OpenModal(ModalKind::QueryHistoryPicker),
        combos: &[KeyCombo::ctrl(Key::Char('o'))],
    };
}

pub const SQL_MODAL_KEYS: &[KeyBinding] = &[
    sql_modal::RUN,
    sql_modal::ESC_NORMAL,
    sql_modal::MOVE,
    sql_modal::HOME_END,
    sql_modal::TAB,
    sql_modal::CLEAR,
    sql_modal::QUERY_HISTORY,
];

pub mod sql_modal_confirming {
    use crate::update::action::Action;
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    // Unconditional only in acknowledge states; typed-name confirmation gates
    // it on input match.
    pub const ENTER_EXECUTE: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute the confirmed statement",
        action: Action::SqlModalConfirmExecute,
        combos: &[KeyCombo::plain(Key::Enter)],
    };

    pub const CANCEL_CONFIRM: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Back",
        description: "Cancel and return to editor",
        action: Action::SqlModalCancelConfirm,
        combos: &[KeyCombo::plain(Key::Esc)],
    };
}

// Confirming states swap out the whole SQL keymap, so their keys are declared
// apart from SQL_MODAL_KEYS.
pub const SQL_MODAL_CONFIRMING_KEYS: &[KeyBinding] = &[
    sql_modal_confirming::ENTER_EXECUTE,
    sql_modal_confirming::CANCEL_CONFIRM,
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
        action: Action::ToggleModal(ModalKind::Help),
        combos: &[],
    },
    KeyBinding {
        key_short: ":sql",
        key: ":sql",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenModal(ModalKind::SqlModal),
        combos: &[],
    },
    KeyBinding {
        key_short: ":erd",
        key: ":erd",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::OpenModal(ModalKind::ErTablePicker),
        combos: &[],
    },
    KeyBinding {
        key_short: ":settings",
        key: ":settings",
        desc_short: "Settings",
        description: "Open Settings",
        action: Action::OpenModal(ModalKind::Settings),
        combos: &[],
    },
    KeyBinding {
        key_short: ":theme",
        key: ":theme",
        desc_short: "Theme",
        description: "Open Theme Settings",
        action: Action::OpenModal(ModalKind::Settings),
        combos: &[],
    },
    KeyBinding {
        key_short: ":palette",
        key: ":palette",
        desc_short: "Palette",
        description: "Open Command Palette",
        action: Action::OpenModal(ModalKind::CommandPalette),
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

pub mod cell_edit {
    use crate::update::action::Action;
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const WRITE: KeyBinding = KeyBinding {
        key_short: ":w",
        key: ":w",
        desc_short: "Write",
        description: "Preview and confirm UPDATE",
        action: Action::SubmitCellEditWrite,
        combos: &[], // :w is a command sequence, not a single combo
    };

    pub const TYPE: KeyBinding = KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Edit",
        description: "Edit cell value",
        action: Action::None,
        combos: &[],
    };

    pub const MOVE: KeyBinding = KeyBinding {
        key_short: "←→",
        key: "←→",
        desc_short: "Move",
        description: "Move cursor",
        action: Action::None,
        combos: &[],
    };

    pub const HOME_END: KeyBinding = KeyBinding {
        key_short: "Home/End",
        key: "Home/End",
        desc_short: "Jump",
        description: "Jump to start/end",
        action: Action::None,
        combos: &[],
    };

    pub const COMMAND: KeyBinding = KeyBinding {
        key_short: ":",
        key: ":",
        desc_short: "Cmd",
        description: "Open command line",
        action: Action::EnterCommandLine,
        combos: &[KeyCombo::plain(Key::Char(':'))],
    };

    pub const ESC_CANCEL: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Exit to Cell Active (draft preserved)",
        action: Action::ResultCancelCellEdit,
        combos: &[KeyCombo::plain(Key::Esc)],
    };
}

pub const CELL_EDIT_KEYS: &[KeyBinding] = &[
    cell_edit::WRITE,
    cell_edit::TYPE,
    cell_edit::MOVE,
    cell_edit::HOME_END,
    cell_edit::COMMAND,
    cell_edit::ESC_CANCEL,
];
