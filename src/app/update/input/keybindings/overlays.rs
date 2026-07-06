use super::{KeyBinding, ModeRow};
use crate::model::shared::settings::KeymapPreset;

// =============================================================================
// Overlays (common display hints)
// =============================================================================

pub mod overlay {
    use crate::update::action::Action;
    use crate::update::input::keybindings::KeyBinding;

    pub const ESC_CANCEL: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Close overlay / Cancel",
        action: Action::None,
        combos: &[],
    };

    pub const ESC_CLOSE: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close overlay",
        action: Action::None,
        combos: &[],
    };

    pub const ENTER_EXECUTE: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        action: Action::None,
        combos: &[],
    };

    pub const ENTER_SELECT: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Confirm selection",
        action: Action::None,
        combos: &[],
    };

    pub const NAVIGATE_JK: KeyBinding = KeyBinding {
        key_short: "^N/^P/j/k/↑↓",
        key: "j / k / Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate items",
        action: Action::None,
        combos: &[],
    };

    pub const TYPE_FILTER: KeyBinding = KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        action: Action::None,
        combos: &[],
    };

    pub const ERROR_OPEN: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Error",
        description: "View error details",
        action: Action::None,
        combos: &[],
    };
}

pub const OVERLAY_KEYS: &[KeyBinding] = &[
    overlay::ESC_CANCEL,
    overlay::ESC_CLOSE,
    overlay::ENTER_EXECUTE,
    overlay::ENTER_SELECT,
    overlay::NAVIGATE_JK,
    overlay::TYPE_FILTER,
    overlay::ERROR_OPEN,
];

// =============================================================================
// Help
// =============================================================================

pub mod help {
    use crate::update::action::{
        Action, InputTarget, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget,
    };
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const SCROLL: ModeRow = ModeRow {
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))],
            },
        ],
    };

    pub const TOP_BOTTOM: ModeRow = ModeRow {
        key_short: "Home/End",
        key: "Home / End",
        desc_short: "Top/Btm",
        description: "Jump to top / bottom",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ToStart,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
    };

    pub const HALF_PAGE: ModeRow = ModeRow {
        key_short: "^D/^U",
        key: "Ctrl+D / Ctrl+U",
        desc_short: "Half Page",
        description: "Scroll half page down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('d'))],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::HalfPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('u'))],
            },
        ],
    };

    pub const FULL_PAGE: ModeRow = ModeRow {
        key_short: "^F/^B/PgDn/Up",
        key: "Ctrl+F / Ctrl+B / PageDown / PageUp",
        desc_short: "Full Page",
        description: "Scroll full page down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                combos: &[
                    KeyCombo::ctrl(Key::Char('f')),
                    KeyCombo::plain(Key::PageDown),
                ],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::FullPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('b')), KeyCombo::plain(Key::PageUp)],
            },
        ],
    };

    pub const H_SCROLL: ModeRow = ModeRow {
        key_short: "←→",
        key: "← / →",
        desc_short: "H-Scroll",
        description: "Scroll left / right",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Left,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Right,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
        ],
    };

    pub const TYPE_FILTER: ModeRow = ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Filter help",
        bindings: &[],
    };

    pub const EDIT_FILTER: ModeRow = ModeRow {
        key_short: "Backspace",
        key: "Backspace",
        desc_short: "Edit",
        description: "Edit filter",
        bindings: &[ExecBinding {
            action: Action::TextBackspace {
                target: InputTarget::HelpFilter,
            },
            combos: &[KeyCombo::plain(Key::Backspace)],
        }],
    };

    pub const ESC_CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close help",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::Help),
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };

    pub const CLOSE: ModeRow = ModeRow {
        key_short: "?",
        key: "?",
        desc_short: "Close",
        description: "Close help",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::Help),
            combos: &[KeyCombo::plain(Key::Char('?'))],
        }],
    };
}

pub const HELP_ROWS: &[ModeRow] = &[
    help::SCROLL,
    help::TOP_BOTTOM,
    help::HALF_PAGE,
    help::FULL_PAGE,
    help::H_SCROLL,
    help::TYPE_FILTER,
    help::EDIT_FILTER,
    help::ESC_CLOSE,
    help::CLOSE,
];

// =============================================================================
// Table Picker
// =============================================================================

pub mod table_picker {
    use crate::update::action::{
        Action, CursorMove, InputTarget, ListMotion, ListTarget, ModalKind,
    };
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const ENTER_SELECT: ModeRow = ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Select table",
        bindings: &[ExecBinding {
            action: Action::ConfirmSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    };

    pub const NAVIGATE: ModeRow = ModeRow {
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::TablePicker,
                    motion: ListMotion::Previous,
                },
                combos: &[KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))],
            },
        ],
    };

    pub const TYPE_FILTER: ModeRow = ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        bindings: &[
            ExecBinding {
                action: Action::TextBackspace {
                    target: InputTarget::Filter,
                },
                combos: &[KeyCombo::plain(Key::Backspace)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::Filter,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::Filter,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::Filter,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::Filter,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
    };

    pub const ESC_CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::TablePicker),
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };
}

pub const TABLE_PICKER_ROWS: &[ModeRow] = &[
    table_picker::ENTER_SELECT,
    table_picker::NAVIGATE,
    table_picker::TYPE_FILTER,
    table_picker::ESC_CLOSE,
];

// =============================================================================
// ER Table Picker
// =============================================================================

pub mod er_picker {
    use crate::update::action::{
        Action, CursorMove, InputTarget, ListMotion, ListTarget, ModalKind,
    };
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const ENTER_GENERATE: ModeRow = ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Generate",
        description: "Generate ER diagram",
        bindings: &[ExecBinding {
            action: Action::ErConfirmSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    };

    pub const SELECT: ModeRow = ModeRow {
        key_short: "Space",
        key: "Space",
        desc_short: "Select",
        description: "Toggle table selection",
        bindings: &[ExecBinding {
            action: Action::ErToggleSelection,
            combos: &[KeyCombo::plain(Key::Char(' '))],
        }],
    };

    pub const SELECT_ALL: ModeRow = ModeRow {
        key_short: "^A",
        key: "Ctrl+A",
        desc_short: "All",
        description: "Select/deselect all tables",
        bindings: &[ExecBinding {
            action: Action::ErSelectAll,
            combos: &[KeyCombo::ctrl(Key::Char('a'))],
        }],
    };

    pub const SELECT_ALL_IDE: ModeRow = ModeRow {
        key_short: "⌥A",
        key: "Alt+A",
        desc_short: "All",
        description: "Select/deselect all tables",
        bindings: &[ExecBinding {
            action: Action::ErSelectAll,
            combos: &[KeyCombo::alt(Key::Char('a'))],
        }],
    };

    pub const NAVIGATE: ModeRow = ModeRow {
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Nav",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::ErTablePicker,
                    motion: ListMotion::Previous,
                },
                combos: &[KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))],
            },
        ],
    };

    pub const TYPE_FILTER: ModeRow = ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        bindings: &[
            ExecBinding {
                action: Action::TextBackspace {
                    target: InputTarget::ErFilter,
                },
                combos: &[KeyCombo::plain(Key::Backspace)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::ErFilter,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::ErFilter,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::ErFilter,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::ErFilter,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
    };

    pub const ESC_CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::ErTablePicker),
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };
}

pub const ER_PICKER_ROWS: &[ModeRow] = &[
    er_picker::ENTER_GENERATE,
    er_picker::SELECT,
    er_picker::SELECT_ALL,
    er_picker::NAVIGATE,
    er_picker::TYPE_FILTER,
    er_picker::ESC_CLOSE,
];

pub const ER_PICKER_ROWS_IDE: &[ModeRow] = &[
    er_picker::ENTER_GENERATE,
    er_picker::SELECT,
    er_picker::SELECT_ALL_IDE,
    er_picker::NAVIGATE,
    er_picker::TYPE_FILTER,
    er_picker::ESC_CLOSE,
];

pub fn er_picker_rows(preset: KeymapPreset) -> &'static [ModeRow] {
    match preset {
        KeymapPreset::Default => ER_PICKER_ROWS,
        KeymapPreset::Ide => ER_PICKER_ROWS_IDE,
    }
}

pub fn er_picker_select_all(preset: KeymapPreset) -> &'static ModeRow {
    match preset {
        KeymapPreset::Default => &er_picker::SELECT_ALL,
        KeymapPreset::Ide => &er_picker::SELECT_ALL_IDE,
    }
}

// =============================================================================
// Query History Picker
// =============================================================================

pub mod query_history_picker {
    use crate::update::action::{
        Action, CursorMove, InputTarget, ListMotion, ListTarget, ModalKind,
    };
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const ENTER_SELECT: ModeRow = ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Select query",
        bindings: &[ExecBinding {
            action: Action::QueryHistoryConfirmSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    };

    pub const NAVIGATE: ModeRow = ModeRow {
        key_short: "^N/^P/↑↓",
        key: "Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Next,
                },
                combos: &[KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::QueryHistory,
                    motion: ListMotion::Previous,
                },
                combos: &[KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))],
            },
        ],
    };

    pub const TYPE_FILTER: ModeRow = ModeRow {
        key_short: "type",
        key: "type",
        desc_short: "Filter",
        description: "Type to filter",
        bindings: &[
            ExecBinding {
                action: Action::TextBackspace {
                    target: InputTarget::QueryHistoryFilter,
                },
                combos: &[KeyCombo::plain(Key::Backspace)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::QueryHistoryFilter,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::QueryHistoryFilter,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::QueryHistoryFilter,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::QueryHistoryFilter,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
    };

    pub const ESC_CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::QueryHistoryPicker),
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };
}

pub const QUERY_HISTORY_PICKER_ROWS: &[ModeRow] = &[
    query_history_picker::ENTER_SELECT,
    query_history_picker::NAVIGATE,
    query_history_picker::TYPE_FILTER,
    query_history_picker::ESC_CLOSE,
];

// =============================================================================
// Command Palette
// =============================================================================

pub mod command_palette {
    use crate::update::action::{Action, ListMotion, ListTarget, ModalKind};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const ENTER_EXECUTE: ModeRow = ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Execute",
        description: "Execute command",
        bindings: &[ExecBinding {
            action: Action::ConfirmSelection,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    };

    pub const NAVIGATE_JK: ModeRow = ModeRow {
        key_short: "^N/^P/j/k/↑↓",
        key: "j / k / Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Navigate",
        description: "Navigate",
        bindings: &[
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::CommandPalette,
                    motion: ListMotion::Next,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('j')),
                    KeyCombo::plain(Key::Down),
                    KeyCombo::ctrl(Key::Char('n')),
                ],
            },
            ExecBinding {
                action: Action::ListSelect {
                    target: ListTarget::CommandPalette,
                    motion: ListMotion::Previous,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::plain(Key::Up),
                    KeyCombo::ctrl(Key::Char('p')),
                ],
            },
        ],
    };

    pub const ESC_CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::CommandPalette),
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };
}

pub const COMMAND_PALETTE_ROWS: &[ModeRow] = &[
    command_palette::ENTER_EXECUTE,
    command_palette::NAVIGATE_JK,
    command_palette::ESC_CLOSE,
];

// =============================================================================
// Settings
// =============================================================================

pub mod settings {
    use crate::update::action::Action;
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const APPLY: ModeRow = ModeRow {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Apply",
        description: "Apply setting",
        bindings: &[ExecBinding {
            action: Action::SettingsApply,
            combos: &[KeyCombo::plain(Key::Enter)],
        }],
    };

    pub const SELECT: ModeRow = ModeRow {
        key_short: "j/k/↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Select",
        description: "Select setting",
        bindings: &[
            ExecBinding {
                action: Action::SettingsSelectNext,
                combos: &[KeyCombo::plain(Key::Down), KeyCombo::plain(Key::Char('j'))],
            },
            ExecBinding {
                action: Action::SettingsSelectPrevious,
                combos: &[KeyCombo::plain(Key::Up), KeyCombo::plain(Key::Char('k'))],
            },
        ],
    };

    pub const EDIT: ModeRow = ModeRow {
        key_short: "i",
        key: "i",
        desc_short: "Edit",
        description: "Edit custom browser",
        bindings: &[ExecBinding {
            action: Action::SettingsStartCustomBrowserEdit,
            combos: &[KeyCombo::plain(Key::Char('i'))],
        }],
    };

    pub const SECTION: ModeRow = ModeRow {
        key_short: "Tab/⇧Tab",
        key: "Tab / Shift+Tab",
        desc_short: "Section",
        description: "Switch settings section",
        bindings: &[
            ExecBinding {
                action: Action::SettingsNextSection,
                combos: &[KeyCombo::plain(Key::Tab)],
            },
            ExecBinding {
                action: Action::SettingsPreviousSection,
                combos: &[KeyCombo::shift(Key::BackTab)],
            },
        ],
    };

    pub const CANCEL: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel",
        bindings: &[ExecBinding {
            action: Action::SettingsCancel,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };
}

pub const SETTINGS_ROWS: &[ModeRow] = &[
    settings::APPLY,
    settings::SELECT,
    settings::EDIT,
    settings::SECTION,
    settings::CANCEL,
];

// =============================================================================
// Confirm Dialog
// =============================================================================

pub mod confirm {
    use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const YES: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Confirm",
        description: "Confirm",
        action: Action::ConfirmDialogConfirm,
        combos: &[KeyCombo::plain(Key::Enter)],
    };

    pub const SCROLL_DOWN: KeyBinding = KeyBinding {
        key_short: "^N/j/↓",
        key: "Ctrl+N / j / ↓",
        desc_short: "Down",
        description: "Scroll down",
        action: Action::Scroll {
            target: ScrollTarget::ConfirmDialog,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        },
        combos: &[
            KeyCombo::plain(Key::Char('j')),
            KeyCombo::plain(Key::Down),
            KeyCombo::ctrl(Key::Char('n')),
        ],
    };

    pub const SCROLL_UP: KeyBinding = KeyBinding {
        key_short: "^P/k/↑",
        key: "Ctrl+P / k / ↑",
        desc_short: "Up",
        description: "Scroll up",
        action: Action::Scroll {
            target: ScrollTarget::ConfirmDialog,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        },
        combos: &[
            KeyCombo::plain(Key::Char('k')),
            KeyCombo::plain(Key::Up),
            KeyCombo::ctrl(Key::Char('p')),
        ],
    };

    pub const NO: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel",
        action: Action::ConfirmDialogCancel,
        combos: &[KeyCombo::plain(Key::Esc)],
    };
}

pub const CONFIRM_DIALOG_KEYS: &[KeyBinding] = &[
    confirm::YES,
    confirm::SCROLL_DOWN,
    confirm::SCROLL_UP,
    confirm::NO,
];

// =============================================================================
// JSONB Detail (Viewing)
// =============================================================================

pub mod jsonb_detail {
    use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const YANK: ModeRow = ModeRow {
        key_short: "y",
        key: "y",
        desc_short: "Copy",
        description: "Copy full JSON",
        bindings: &[ExecBinding {
            action: Action::JsonbYankAll,
            combos: &[KeyCombo::plain(Key::Char('y'))],
        }],
    };

    pub const INSERT: ModeRow = ModeRow {
        key_short: "i",
        key: "i / A",
        desc_short: "Insert",
        description: "Enter Insert mode / append at line end",
        bindings: &[
            ExecBinding {
                action: Action::JsonbEnterEdit,
                combos: &[KeyCombo::plain(Key::Char('i'))],
            },
            ExecBinding {
                action: Action::JsonbAppendInsert,
                combos: &[KeyCombo::plain(Key::Char('A'))],
            },
        ],
    };

    pub const SEARCH: ModeRow = ModeRow {
        key_short: "/",
        key: "/",
        desc_short: "Search",
        description: "Search JSON text",
        bindings: &[ExecBinding {
            action: Action::JsonbEnterSearch,
            combos: &[KeyCombo::plain(Key::Char('/'))],
        }],
    };

    pub const NEXT_PREV: ModeRow = ModeRow {
        key_short: "n/N",
        key: "n / N",
        desc_short: "Next/Prev",
        description: "Jump to next / previous search result",
        bindings: &[
            ExecBinding {
                action: Action::JsonbSearchNext,
                combos: &[KeyCombo::plain(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::JsonbSearchPrev,
                combos: &[KeyCombo::plain(Key::Char('N'))],
            },
        ],
    };

    pub const MOVE: ModeRow = ModeRow {
        key_short: "hjkl/↑↓←→",
        key: "h / j / k / l / ↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Char('h')), KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Char('l')), KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Down,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('j')),
                    KeyCombo::ctrl(Key::Char('n')),
                    KeyCombo::plain(Key::Down),
                ],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Up,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::ctrl(Key::Char('p')),
                    KeyCombo::plain(Key::Up),
                ],
            },
        ],
    };

    pub const JUMP: ModeRow = ModeRow {
        key_short: "0$wb",
        key: "0 / $ / w / b / Home / End",
        desc_short: "Jump",
        description: "Move by word or line boundary",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LineStart,
                },
                combos: &[KeyCombo::plain(Key::Char('0'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LineEnd,
                },
                combos: &[KeyCombo::plain(Key::Char('$'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::WordForward,
                },
                combos: &[KeyCombo::plain(Key::Char('w'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::WordBackward,
                },
                combos: &[KeyCombo::plain(Key::Char('b'))],
            },
        ],
    };

    pub const VIEW: ModeRow = ModeRow {
        key_short: "ggGHML",
        key: "gg / G / H / M / L",
        desc_short: "View",
        description: "Jump by buffer or viewport",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::LastLine,
                },
                combos: &[KeyCombo::plain(Key::Char('G'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::ViewportTop,
                },
                combos: &[KeyCombo::plain(Key::Char('H'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::ViewportMiddle,
                },
                combos: &[KeyCombo::plain(Key::Char('M'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::ViewportBottom,
                },
                combos: &[KeyCombo::plain(Key::Char('L'))],
            },
        ],
    };

    pub const CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc / q",
        desc_short: "Close",
        description: "Close JSONB detail",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::JsonbDetail),
            combos: &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('q'))],
        }],
    };
}

pub const JSONB_DETAIL_ROWS: &[ModeRow] = &[
    jsonb_detail::YANK,
    jsonb_detail::INSERT,
    jsonb_detail::SEARCH,
    jsonb_detail::NEXT_PREV,
    jsonb_detail::MOVE,
    jsonb_detail::JUMP,
    jsonb_detail::VIEW,
    jsonb_detail::CLOSE,
];

// =============================================================================
// Row Detail
// =============================================================================

pub mod row_detail {
    use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const YANK: ModeRow = ModeRow {
        key_short: "y",
        key: "y",
        desc_short: "Copy",
        description: "Copy displayed text to clipboard",
        bindings: &[ExecBinding {
            action: Action::RowDetailYank,
            combos: &[KeyCombo::plain(Key::Char('y'))],
        }],
    };

    pub const YANK_JSON: ModeRow = ModeRow {
        key_short: "Y",
        key: "Y",
        desc_short: "Copy JSON",
        description: "Copy row as JSON to clipboard",
        bindings: &[ExecBinding {
            action: Action::RowDetailYankJson,
            combos: &[KeyCombo::plain(Key::Char('Y'))],
        }],
    };

    pub const SCROLL: ModeRow = ModeRow {
        key_short: "j/k/⇟/⇞/\u{2303}F/\u{2303}B",
        key: "j / k / PageDown / PageUp / Ctrl+F / Ctrl+B",
        desc_short: "Scroll",
        description: "Scroll by line or page",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Char('j')), KeyCombo::plain(Key::Down)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Char('k')), KeyCombo::plain(Key::Up)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                combos: &[
                    KeyCombo::plain(Key::PageDown),
                    KeyCombo::ctrl(Key::Char('f')),
                ],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::FullPage,
                },
                combos: &[KeyCombo::plain(Key::PageUp), KeyCombo::ctrl(Key::Char('b'))],
            },
        ],
    };

    pub const HALF_PAGE: ModeRow = ModeRow {
        key_short: "\u{2303}D/\u{2303}U",
        key: "Ctrl+D / Ctrl+U",
        desc_short: "Half Page",
        description: "Scroll half page down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('d'))],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::HalfPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('u'))],
            },
        ],
    };

    pub const JUMP: ModeRow = ModeRow {
        key_short: "g/G",
        key: "g / G / Home / End",
        desc_short: "Top/Btm",
        description: "Jump to top / bottom",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ToStart,
                },
                combos: &[KeyCombo::plain(Key::Char('g')), KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::RowDetail,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                combos: &[KeyCombo::plain(Key::Char('G')), KeyCombo::plain(Key::End)],
            },
        ],
    };

    pub const CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Close",
        description: "Close Row Detail modal",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::RowDetail),
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };
}

pub const ROW_DETAIL_ROWS: &[ModeRow] = &[
    row_detail::YANK,
    row_detail::YANK_JSON,
    row_detail::SCROLL,
    row_detail::HALF_PAGE,
    row_detail::JUMP,
    row_detail::CLOSE,
];

// =============================================================================
// JSONB Search (active search input)
// =============================================================================

pub mod jsonb_search {
    use crate::update::action::Action;
    use crate::update::input::keybindings::{Key, KeyBinding, KeyCombo};

    pub const TYPE_SEARCH: KeyBinding = KeyBinding {
        key_short: "type",
        key: "type",
        desc_short: "Search",
        description: "Type to search",
        action: Action::None,
        combos: &[],
    };

    pub const CONFIRM: KeyBinding = KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Confirm",
        description: "Confirm search",
        action: Action::JsonbSearchSubmit,
        combos: &[KeyCombo::plain(Key::Enter)],
    };

    pub const CANCEL: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel search",
        action: Action::JsonbExitSearch,
        combos: &[KeyCombo::plain(Key::Esc)],
    };
}

pub const JSONB_SEARCH_KEYS: &[KeyBinding] = &[
    jsonb_search::TYPE_SEARCH,
    jsonb_search::CONFIRM,
    jsonb_search::CANCEL,
];

// =============================================================================
// JSONB Edit
// =============================================================================

pub mod jsonb_edit {
    use crate::update::action::{Action, CursorMove, InputTarget};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const ESC_NORMAL: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Return to Normal mode",
        bindings: &[ExecBinding {
            action: Action::JsonbExitEdit,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
    };

    pub const MOVE: ModeRow = ModeRow {
        key_short: "↑↓←→",
        key: "↑↓←→",
        desc_short: "Move",
        description: "Move cursor",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Up,
                },
                combos: &[KeyCombo::plain(Key::Up)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Down,
                },
                combos: &[KeyCombo::plain(Key::Down)],
            },
        ],
    };

    pub const HOME_END: ModeRow = ModeRow {
        key_short: "Home/End",
        key: "Home / End",
        desc_short: "Line",
        description: "Line start/end",
        bindings: &[
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonbEdit,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
    };
}

pub const JSONB_EDIT_ROWS: &[ModeRow] = &[
    jsonb_edit::ESC_NORMAL,
    jsonb_edit::MOVE,
    jsonb_edit::HOME_END,
];
