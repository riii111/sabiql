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
