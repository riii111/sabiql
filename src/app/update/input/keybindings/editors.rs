use super::KeyBinding;
use crate::model::shared::settings::KeymapPreset;

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

    pub const QUERY_HISTORY_IDE: KeyBinding = KeyBinding {
        key_short: "O",
        key: "O",
        desc_short: "History",
        description: "Open Query History",
        action: Action::OpenModal(ModalKind::QueryHistoryPicker),
        combos: &[KeyCombo::plain(Key::Char('O'))],
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

    pub const EXPLAIN_IDE: KeyBinding = KeyBinding {
        key_short: "E",
        key: "E",
        desc_short: "Explain",
        description: "Run EXPLAIN on current query",
        action: Action::ExplainRequest,
        combos: &[KeyCombo::plain(Key::Char('E'))],
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

    pub const EXPLAIN_IDE: KeyBinding = KeyBinding {
        key_short: "E",
        key: "E",
        desc_short: "Explain",
        description: "Run EXPLAIN on current query",
        action: Action::ExplainRequest,
        combos: &[KeyCombo::plain(Key::Char('E'))],
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

pub fn sql_modal_normal_query_history(preset: KeymapPreset) -> &'static KeyBinding {
    match preset {
        KeymapPreset::Default => &sql_modal_normal::QUERY_HISTORY,
        KeymapPreset::Ide => &sql_modal_normal::QUERY_HISTORY_IDE,
    }
}

pub fn sql_modal_plan_explain(preset: KeymapPreset) -> &'static KeyBinding {
    match preset {
        KeymapPreset::Default => &sql_modal_plan::EXPLAIN,
        KeymapPreset::Ide => &sql_modal_plan::EXPLAIN_IDE,
    }
}

pub fn sql_modal_compare_explain(preset: KeymapPreset) -> &'static KeyBinding {
    match preset {
        KeymapPreset::Default => &sql_modal_compare::EXPLAIN,
        KeymapPreset::Ide => &sql_modal_compare::EXPLAIN_IDE,
    }
}

// =============================================================================
// Cell Edit
// =============================================================================

pub mod cell_edit {
    use crate::update::action::Action;
    use crate::update::input::keybindings::KeyBinding;

    pub const WRITE: KeyBinding = KeyBinding {
        key_short: ":w",
        key: ":w",
        desc_short: "Write",
        description: "Preview and confirm UPDATE",
        action: Action::SubmitCellEditWrite,
        combos: &[], // :w is a command sequence, not a single combo
    };
}
