use super::KeyBinding;
use super::{Key, KeyCombo};
use crate::update::action::Action;

// =============================================================================
// Global Keys (Normal mode)
// =============================================================================

pub mod global {
    use crate::update::action::{Action, ModalKind};
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const QUIT: KeyBinding = KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
        combos: &[KeyCombo::plain(Key::Char('q'))],
    };

    pub const HELP: KeyBinding = KeyBinding {
        key_short: "?",
        key: "?",
        desc_short: "Help",
        description: "Toggle help",
        action: Action::ToggleModal(ModalKind::Help),
        combos: &[KeyCombo::plain(Key::Char('?'))],
    };

    pub const TABLE_PICKER: KeyBinding = KeyBinding {
        key_short: "^P",
        key: "Ctrl+P",
        desc_short: "Tables",
        description: "Open Table Picker",
        action: Action::OpenModal(ModalKind::TablePicker),
        combos: &[KeyCombo::ctrl(Key::Char('p'))],
    };

    pub const SETTINGS: KeyBinding = KeyBinding {
        key_short: "^K",
        key: "Ctrl+K",
        desc_short: "Settings",
        description: "Open Settings",
        action: Action::OpenModal(ModalKind::Settings),
        combos: &[KeyCombo::ctrl(Key::Char('k'))],
    };

    pub const COMMAND_LINE: KeyBinding = KeyBinding {
        key_short: ":",
        key: ":",
        desc_short: "Cmd",
        description: "Enter command line",
        action: Action::EnterCommandLine,
        combos: &[KeyCombo::plain(Key::Char(':'))],
    };

    pub const COMMAND_PALETTE: KeyBinding = KeyBinding {
        key_short: "F1",
        key: "F1",
        desc_short: "Palette",
        description: "Open Command Palette",
        action: Action::OpenModal(ModalKind::CommandPalette),
        combos: &[KeyCombo::plain(Key::F(1))],
    };

    // FOCUS / EXIT_FOCUS share the same combo for the same Action::ToggleFocus.
    // Two entries exist because the footer shows different labels depending on
    // whether focus mode is active.
    pub const FOCUS: KeyBinding = KeyBinding {
        key_short: "f",
        key: "f",
        desc_short: "Focus",
        description: "Toggle Focus mode",
        action: Action::ToggleFocus,
        combos: &[KeyCombo::plain(Key::Char('f'))],
    };

    pub const EXIT_FOCUS: KeyBinding = KeyBinding {
        key_short: "f",
        key: "f",
        desc_short: "Exit Focus",
        description: "Exit Focus mode",
        action: Action::ToggleFocus,
        combos: &[KeyCombo::plain(Key::Char('f'))],
    };

    pub const PANE_SWITCH: KeyBinding = KeyBinding {
        key_short: "1/2/3",
        key: "1/2/3",
        desc_short: "Pane",
        description: "Switch pane focus",
        action: Action::None,
        combos: &[],
    };

    pub const INSPECTOR_TABS: KeyBinding = KeyBinding {
        key_short: "Tab/⇧Tab",
        key: "Tab/⇧Tab",
        desc_short: "InsTabs",
        description: "Inspector prev/next tab",
        action: Action::None,
        combos: &[],
    };

    pub const RELOAD: KeyBinding = KeyBinding {
        key_short: "r",
        key: "r",
        desc_short: "Reload",
        description: "Reload metadata",
        action: Action::ReloadMetadata,
        combos: &[KeyCombo::plain(Key::Char('r'))],
    };

    pub const SQL: KeyBinding = KeyBinding {
        key_short: "s",
        key: "s",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenModal(ModalKind::SqlModal),
        combos: &[KeyCombo::plain(Key::Char('s'))],
    };

    pub const ER_DIAGRAM: KeyBinding = KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::OpenModal(ModalKind::ErTablePicker),
        combos: &[KeyCombo::plain(Key::Char('e'))],
    };

    pub const CONNECTIONS: KeyBinding = KeyBinding {
        key_short: "c",
        key: "c",
        desc_short: "Connections",
        description: "Open Connection Selector",
        action: Action::OpenModal(ModalKind::ConnectionSelector),
        combos: &[KeyCombo::plain(Key::Char('c'))],
    };

    pub const CSV_EXPORT: KeyBinding = KeyBinding {
        key_short: "^E",
        key: "Ctrl+E",
        desc_short: "Export",
        description: "Export result to CSV",
        action: Action::RequestCsvExport,
        combos: &[KeyCombo::ctrl(Key::Char('e'))],
    };

    // READ_ONLY / EXIT_READ_ONLY share the same combo for the same
    // Action::ToggleReadOnly. Two entries exist because the footer shows
    // different labels depending on whether read-only mode is active.
    pub const READ_ONLY: KeyBinding = KeyBinding {
        key_short: "^R",
        key: "Ctrl+R",
        desc_short: "Read-Only",
        description: "Enable Read-Only mode",
        action: Action::ToggleReadOnly,
        combos: &[KeyCombo::ctrl(Key::Char('r'))],
    };

    pub const EXIT_READ_ONLY: KeyBinding = KeyBinding {
        key_short: "^R",
        key: "Ctrl+R",
        desc_short: "Read-Write",
        description: "Disable Read-Only mode",
        action: Action::ToggleReadOnly,
        combos: &[KeyCombo::ctrl(Key::Char('r'))],
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

pub const GLOBAL_KEYS: &[KeyBinding] = &[
    global::QUIT,
    global::HELP,
    global::TABLE_PICKER,
    global::SETTINGS,
    global::COMMAND_LINE,
    global::COMMAND_PALETTE,
    global::FOCUS,
    global::EXIT_FOCUS,
    global::PANE_SWITCH,
    global::INSPECTOR_TABS,
    global::RELOAD,
    global::SQL,
    global::ER_DIAGRAM,
    global::CONNECTIONS,
    global::CSV_EXPORT,
    global::READ_ONLY,
    global::EXIT_READ_ONLY,
    global::QUERY_HISTORY,
];

pub const NAVIGATION_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "j/↓",
        key: "j / ↓",
        desc_short: "Down",
        description: "Move down / scroll",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "k/↑",
        key: "k / ↑",
        desc_short: "Up",
        description: "Move up / scroll",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "g",
        key: "g / Home",
        desc_short: "Top",
        description: "First item / top",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "G",
        key: "G / End",
        desc_short: "Bottom",
        description: "Last item / bottom",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "H",
        key: "H",
        desc_short: "Viewport Top",
        description: "First visible item",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "M",
        key: "M",
        desc_short: "Viewport Mid",
        description: "Middle of visible items",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "L",
        key: "L",
        desc_short: "Viewport Btm",
        description: "Last visible item",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "zz/zt/zb",
        key: "zz / zt / zb",
        desc_short: "Scroll To",
        description: "Scroll cursor to center/top/bottom",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "^D/^U",
        key: "Ctrl+D / Ctrl+U",
        desc_short: "Half Page",
        description: "Scroll half page down/up",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "^F/^B",
        key: "Ctrl+F/B / PgDn/Up",
        desc_short: "Full Page",
        description: "Scroll full page down/up",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "]",
        key: "]",
        desc_short: "Next Page",
        description: "Next page (Preview)",
        action: Action::ResultNextPage,
        combos: &[KeyCombo::plain(Key::Char(']'))],
    },
    KeyBinding {
        key_short: "[",
        key: "[",
        desc_short: "Prev Page",
        description: "Previous page (Preview)",
        action: Action::ResultPrevPage,
        combos: &[KeyCombo::plain(Key::Char('['))],
    },
];

pub mod footer_nav {
    use crate::update::action::Action;
    use crate::update::input::keybindings::KeyBinding;

    pub const SCROLL: KeyBinding = KeyBinding {
        key_short: "j/↓",
        key: "j / ↓",
        desc_short: "Scroll",
        description: "Move down/up",
        action: Action::None,
        combos: &[],
    };

    pub const SCROLL_SHORT: KeyBinding = KeyBinding {
        key_short: "k/↑",
        key: "k / ↑",
        desc_short: "Scroll",
        description: "Move down/up",
        action: Action::None,
        combos: &[],
    };

    pub const TOP_BOTTOM: KeyBinding = KeyBinding {
        key_short: "g/G H/M/L",
        key: "g / G / H / M / L",
        desc_short: "Top/Bot/Viewport",
        description: "First/Last item, Viewport top/mid/bot",
        action: Action::None,
        combos: &[],
    };

    pub const H_SCROLL: KeyBinding = KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l / ← / →",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
        combos: &[],
    };

    pub const PAGE_NAV: KeyBinding = KeyBinding {
        key_short: "]/[",
        key: "] / [",
        desc_short: "Page",
        description: "Next/Previous page",
        action: Action::None,
        combos: &[],
    };
}

pub const FOOTER_NAV_KEYS: &[KeyBinding] = &[
    footer_nav::SCROLL,
    footer_nav::SCROLL_SHORT,
    footer_nav::TOP_BOTTOM,
    footer_nav::H_SCROLL,
    footer_nav::PAGE_NAV,
];

pub mod result_active {
    use crate::update::action::Action;
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const ENTER_DEEPEN: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Enter cell selection at the current viewport anchor",
        action: Action::ResultActivateCell,
        combos: &[KeyCombo::plain(Key::Enter)],
    };

    pub const YANK: KeyBinding = KeyBinding {
        key_short: "Y",
        key: "Y",
        desc_short: "Yank Cell",
        description: "Copy the active cell value to clipboard",
        action: Action::ResultCellYank,
        combos: &[KeyCombo::plain(Key::Char('Y'))],
    };

    pub const STAGE_DELETE: KeyBinding = KeyBinding {
        key_short: "dd",
        key: "d, d",
        desc_short: "Stage Del",
        description: "Stage the active row for deletion (red highlight; :w to commit)",
        action: Action::StageRowForDelete,
        combos: &[], // dd is a two-key sequence, not a single combo
    };

    pub const UNSTAGE_DELETE: KeyBinding = KeyBinding {
        key_short: "u",
        key: "u",
        desc_short: "Unstage",
        description: "Unstage the last staged row deletion",
        action: Action::UnstageLastStagedRow,
        combos: &[KeyCombo::plain(Key::Char('u'))],
    };

    pub const CELL_NAV: KeyBinding = KeyBinding {
        key_short: "h/l",
        key: "h / l",
        desc_short: "Cell",
        description: "Move cell left/right",
        action: Action::None,
        combos: &[],
    };

    pub const ROW_NAV: KeyBinding = KeyBinding {
        key_short: "j/k",
        key: "j / k",
        desc_short: "Row",
        description: "Move the active row up/down",
        action: Action::None,
        combos: &[],
    };

    pub const TOP_BOTTOM: KeyBinding = KeyBinding {
        key_short: "g/G H/M/L",
        key: "g / G / H / M / L",
        desc_short: "Top/Bot/Viewport",
        description: "Jump the active row to first/last or viewport top/mid/bot",
        action: Action::None,
        combos: &[],
    };

    pub const ESC_BACK: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Back",
        description: "Exit cell selection and return to scroll mode without clearing staged deletes",
        action: Action::ResultExitToScroll,
        combos: &[KeyCombo::plain(Key::Esc)],
    };

    pub const EDIT: KeyBinding = KeyBinding {
        key_short: "i",
        key: "i",
        desc_short: "Edit",
        description: "Edit active cell",
        action: Action::ResultEnterCellEdit,
        combos: &[KeyCombo::plain(Key::Char('i'))],
    };

    pub const DRAFT_DISCARD: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Discard",
        description: "Discard the pending draft and stay in cell selection",
        action: Action::ResultDiscardCellEdit,
        combos: &[KeyCombo::plain(Key::Esc)],
    };

    pub const ROW_YANK: KeyBinding = KeyBinding {
        key_short: "yy",
        key: "y, y",
        desc_short: "Yank Row",
        description: "Copy the active row values to clipboard (TSV)",
        action: Action::ResultRowYank,
        combos: &[],
    };
}

pub const RESULT_ACTIVE_KEYS: &[KeyBinding] = &[
    result_active::ENTER_DEEPEN,
    result_active::YANK,
    result_active::STAGE_DELETE,
    result_active::UNSTAGE_DELETE,
    result_active::CELL_NAV,
    result_active::ROW_NAV,
    result_active::TOP_BOTTOM,
    result_active::ESC_BACK,
    result_active::EDIT,
    result_active::DRAFT_DISCARD,
    result_active::ROW_YANK,
];

pub mod history {
    use crate::update::action::Action;
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const OPEN: KeyBinding = KeyBinding {
        key_short: "^H",
        key: "Ctrl+H",
        desc_short: "History",
        description: "Toggle Result History",
        action: Action::OpenResultHistory,
        combos: &[KeyCombo::ctrl(Key::Char('h'))],
    };

    pub const NAV: KeyBinding = KeyBinding {
        key_short: "]/[",
        key: "] / [",
        desc_short: "History",
        description: "Navigate history newer/older",
        action: Action::None,
        combos: &[],
    };

    pub const EXIT: KeyBinding = KeyBinding {
        key_short: "^H",
        key: "Ctrl+H",
        desc_short: "Back",
        description: "Exit history (back to latest)",
        action: Action::None,
        combos: &[],
    };
}

pub const HISTORY_KEYS: &[KeyBinding] = &[history::OPEN, history::NAV, history::EXIT];

pub mod inspector_ddl {
    use crate::update::action::Action;
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const YANK: KeyBinding = KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy DDL to clipboard",
        action: Action::DdlYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    };
}

pub const INSPECTOR_DDL_KEYS: &[KeyBinding] = &[inspector_ddl::YANK];
