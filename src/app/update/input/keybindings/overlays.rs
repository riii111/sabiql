use super::{KeyBinding, ModeRow};

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
        key_short: "^N/^P/jk/↑↓",
        key: "Ctrl+N / Ctrl+P / j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                combos: &[
                    KeyCombo::plain(Key::Down),
                    KeyCombo::plain(Key::Char('j')),
                    KeyCombo::ctrl(Key::Char('n')),
                ],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                combos: &[
                    KeyCombo::plain(Key::Up),
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::ctrl(Key::Char('p')),
                ],
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
        key_short: "h/l/←/→",
        key: "h / l / ← / →",
        desc_short: "H-Scroll",
        description: "Scroll left / right",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Left,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Char('h')), KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::Help,
                    direction: ScrollDirection::Right,
                    amount: ScrollAmount::Line,
                },
                combos: &[KeyCombo::plain(Key::Char('l')), KeyCombo::plain(Key::Right)],
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

    pub const START_FILTER: ModeRow = ModeRow {
        key_short: "/",
        key: "/",
        desc_short: "Filter",
        description: "Edit filter",
        bindings: &[ExecBinding {
            action: Action::EnterHelpFilter,
            combos: &[KeyCombo::plain(Key::Char('/'))],
        }],
    };

    pub const ESC_VIEWING: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Browse",
        description: "Return to help browsing",
        bindings: &[ExecBinding {
            action: Action::ExitHelpFilter,
            combos: &[KeyCombo::plain(Key::Esc)],
        }],
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
    help::START_FILTER,
    help::ESC_CLOSE,
    help::CLOSE,
];

pub const HELP_VIEWING_ROWS: &[ModeRow] = &[
    help::SCROLL,
    help::TOP_BOTTOM,
    help::HALF_PAGE,
    help::FULL_PAGE,
    help::H_SCROLL,
    help::START_FILTER,
    help::ESC_CLOSE,
    help::CLOSE,
];

pub const HELP_EDITING_ROWS: &[ModeRow] = &[help::ESC_VIEWING, help::EDIT_FILTER];

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
// JSON Detail (Viewing)
// =============================================================================

pub mod json_detail {
    use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};
    use crate::update::action::{DetailTarget, SearchStep};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const YANK: ModeRow = ModeRow {
        key_short: "y",
        key: "y",
        desc_short: "Copy",
        description: "Copy full JSON",
        bindings: &[ExecBinding {
            action: Action::DetailYankAll(DetailTarget::JsonDetail),
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
                action: Action::JsonEnterEdit,
                combos: &[KeyCombo::plain(Key::Char('i'))],
            },
            ExecBinding {
                action: Action::JsonAppendInsert,
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
            action: Action::DetailSearch {
                target: DetailTarget::JsonDetail,
                step: SearchStep::Enter,
            },
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
                action: Action::DetailSearch {
                    target: DetailTarget::JsonDetail,
                    step: SearchStep::Next,
                },
                combos: &[KeyCombo::plain(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::DetailSearch {
                    target: DetailTarget::JsonDetail,
                    step: SearchStep::Prev,
                },
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
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Char('h')), KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Char('l')), KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
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
                    target: InputTarget::JsonEdit,
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
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::LineStart,
                },
                combos: &[KeyCombo::plain(Key::Char('0'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::LineEnd,
                },
                combos: &[KeyCombo::plain(Key::Char('$'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::WordForward,
                },
                combos: &[KeyCombo::plain(Key::Char('w'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
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
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::LastLine,
                },
                combos: &[KeyCombo::plain(Key::Char('G'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::ViewportTop,
                },
                combos: &[KeyCombo::plain(Key::Char('H'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::ViewportMiddle,
                },
                combos: &[KeyCombo::plain(Key::Char('M'))],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
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
        description: "Close JSON detail",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::JsonDetail),
            combos: &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('q'))],
        }],
    };
}

pub const JSON_DETAIL_ROWS: &[ModeRow] = &[
    json_detail::YANK,
    json_detail::INSERT,
    json_detail::SEARCH,
    json_detail::NEXT_PREV,
    json_detail::MOVE,
    json_detail::JUMP,
    json_detail::VIEW,
    json_detail::CLOSE,
];

// =============================================================================
// Cell Detail
// =============================================================================

pub mod cell_detail {
    use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
    use crate::update::action::{DetailTarget, SearchStep};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const YANK: ModeRow = ModeRow {
        key_short: "y",
        key: "y",
        desc_short: "Copy",
        description: "Copy full cell text",
        bindings: &[ExecBinding {
            action: Action::DetailYankAll(DetailTarget::CellDetail),
            combos: &[KeyCombo::plain(Key::Char('y'))],
        }],
    };

    pub const SEARCH: ModeRow = ModeRow {
        key_short: "/",
        key: "/",
        desc_short: "Search",
        description: "Search cell text",
        bindings: &[ExecBinding {
            action: Action::DetailSearch {
                target: DetailTarget::CellDetail,
                step: SearchStep::Enter,
            },
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
                action: Action::DetailSearch {
                    target: DetailTarget::CellDetail,
                    step: SearchStep::Next,
                },
                combos: &[KeyCombo::plain(Key::Char('n'))],
            },
            ExecBinding {
                action: Action::DetailSearch {
                    target: DetailTarget::CellDetail,
                    step: SearchStep::Prev,
                },
                combos: &[KeyCombo::plain(Key::Char('N'))],
            },
        ],
    };

    pub const SCROLL: ModeRow = ModeRow {
        key_short: "j/k/↑↓",
        key: "j / k / Ctrl+N / Ctrl+P / ↑ / ↓",
        desc_short: "Scroll",
        description: "Scroll text down / up",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::CellDetail,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::Line,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('j')),
                    KeyCombo::ctrl(Key::Char('n')),
                    KeyCombo::plain(Key::Down),
                ],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::CellDetail,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::Line,
                },
                combos: &[
                    KeyCombo::plain(Key::Char('k')),
                    KeyCombo::ctrl(Key::Char('p')),
                    KeyCombo::plain(Key::Up),
                ],
            },
        ],
    };

    pub const PAGE: ModeRow = ModeRow {
        key_short: "^D/^U Pg",
        key: "Ctrl+D / Ctrl+U / PageDown / PageUp",
        desc_short: "Page",
        description: "Scroll by page",
        bindings: &[
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::CellDetail,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('d'))],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::CellDetail,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::HalfPage,
                },
                combos: &[KeyCombo::ctrl(Key::Char('u'))],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::CellDetail,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                combos: &[KeyCombo::plain(Key::PageDown)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::CellDetail,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::FullPage,
                },
                combos: &[KeyCombo::plain(Key::PageUp)],
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
                    target: ScrollTarget::CellDetail,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ToStart,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::Scroll {
                    target: ScrollTarget::CellDetail,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
    };

    pub const CLOSE: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc / q",
        desc_short: "Close",
        description: "Close cell detail",
        bindings: &[ExecBinding {
            action: Action::CloseModal(ModalKind::CellDetail),
            combos: &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('q'))],
        }],
    };
}

pub const CELL_DETAIL_ROWS: &[ModeRow] = &[
    cell_detail::YANK,
    cell_detail::SEARCH,
    cell_detail::NEXT_PREV,
    cell_detail::SCROLL,
    cell_detail::PAGE,
    cell_detail::TOP_BOTTOM,
    cell_detail::CLOSE,
];

// =============================================================================
// JSON Search (active search input)
// =============================================================================

pub mod json_search {
    use crate::update::action::Action;
    use crate::update::action::{DetailTarget, SearchStep};
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
        action: Action::DetailSearch {
            target: DetailTarget::JsonDetail,
            step: SearchStep::Submit,
        },
        combos: &[KeyCombo::plain(Key::Enter)],
    };

    pub const CANCEL: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel search",
        action: Action::DetailSearch {
            target: DetailTarget::JsonDetail,
            step: SearchStep::Exit,
        },
        combos: &[KeyCombo::plain(Key::Esc)],
    };
}

pub const JSON_SEARCH_KEYS: &[KeyBinding] = &[
    json_search::TYPE_SEARCH,
    json_search::CONFIRM,
    json_search::CANCEL,
];

pub mod cell_detail_search {
    use crate::update::action::Action;
    use crate::update::action::{DetailTarget, SearchStep};
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
        action: Action::DetailSearch {
            target: DetailTarget::CellDetail,
            step: SearchStep::Submit,
        },
        combos: &[KeyCombo::plain(Key::Enter)],
    };

    pub const CANCEL: KeyBinding = KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Cancel",
        description: "Cancel search",
        action: Action::DetailSearch {
            target: DetailTarget::CellDetail,
            step: SearchStep::Exit,
        },
        combos: &[KeyCombo::plain(Key::Esc)],
    };
}

pub const CELL_DETAIL_SEARCH_KEYS: &[KeyBinding] = &[
    cell_detail_search::TYPE_SEARCH,
    cell_detail_search::CONFIRM,
    cell_detail_search::CANCEL,
];

// =============================================================================
// JSON Edit
// =============================================================================

pub mod json_edit {
    use crate::update::action::{Action, CursorMove, InputTarget};
    use crate::update::input::keybindings::{ExecBinding, Key, KeyCombo, ModeRow};

    pub const ESC_NORMAL: ModeRow = ModeRow {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Normal",
        description: "Return to Normal mode",
        bindings: &[ExecBinding {
            action: Action::JsonExitEdit,
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
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Left,
                },
                combos: &[KeyCombo::plain(Key::Left)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Right,
                },
                combos: &[KeyCombo::plain(Key::Right)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Up,
                },
                combos: &[KeyCombo::plain(Key::Up)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
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
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::Home,
                },
                combos: &[KeyCombo::plain(Key::Home)],
            },
            ExecBinding {
                action: Action::TextMoveCursor {
                    target: InputTarget::JsonEdit,
                    direction: CursorMove::End,
                },
                combos: &[KeyCombo::plain(Key::End)],
            },
        ],
    };
}

pub const JSON_EDIT_ROWS: &[ModeRow] =
    &[json_edit::ESC_NORMAL, json_edit::MOVE, json_edit::HOME_END];
