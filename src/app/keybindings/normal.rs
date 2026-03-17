use super::KeyBinding;
use super::types::{Key, KeyCombo};
use crate::app::action::Action;

// =============================================================================
// Global Keys (Normal mode)
// =============================================================================

pub const GLOBAL_KEYS: &[KeyBinding] = &[
    // idx 0: QUIT
    KeyBinding {
        key_short: "q",
        key: "q",
        desc_short: "Quit",
        description: "Quit application",
        action: Action::Quit,
        combos: &[KeyCombo::plain(Key::Char('q'))],
    },
    // idx 1: HELP
    KeyBinding {
        key_short: "?",
        key: "?",
        desc_short: "Help",
        description: "Toggle help",
        action: Action::OpenHelp,
        combos: &[KeyCombo::plain(Key::Char('?'))],
    },
    // idx 2: TABLE_PICKER
    KeyBinding {
        key_short: "^P",
        key: "Ctrl+P",
        desc_short: "Tables",
        description: "Open Table Picker",
        action: Action::OpenTablePicker,
        combos: &[KeyCombo::ctrl(Key::Char('p'))],
    },
    // idx 3: PALETTE
    KeyBinding {
        key_short: "^K",
        key: "Ctrl+K",
        desc_short: "Palette",
        description: "Open Command Palette",
        action: Action::OpenCommandPalette,
        combos: &[KeyCombo::ctrl(Key::Char('k'))],
    },
    // idx 4: COMMAND_LINE
    KeyBinding {
        key_short: ":",
        key: ":",
        desc_short: "Cmd",
        description: "Enter command line",
        action: Action::EnterCommandLine,
        combos: &[KeyCombo::plain(Key::Char(':'))],
    },
    // idx 5: FOCUS / idx 6: EXIT_FOCUS — intentionally duplicate combo ('f')
    // for the same Action::ToggleFocus. Two entries exist because the footer
    // shows different labels depending on whether focus mode is active.
    KeyBinding {
        key_short: "f",
        key: "f",
        desc_short: "Focus",
        description: "Toggle Focus mode",
        action: Action::ToggleFocus,
        combos: &[KeyCombo::plain(Key::Char('f'))],
    },
    KeyBinding {
        key_short: "f",
        key: "f",
        desc_short: "Exit Focus",
        description: "Exit Focus mode",
        action: Action::ToggleFocus,
        combos: &[KeyCombo::plain(Key::Char('f'))],
    },
    // idx 7: PANE_SWITCH
    KeyBinding {
        key_short: "1/2/3",
        key: "1/2/3",
        desc_short: "Pane",
        description: "Switch pane focus",
        action: Action::None,
        combos: &[],
    },
    // idx 8: INSPECTOR_TABS
    KeyBinding {
        key_short: "Tab/⇧Tab",
        key: "Tab/⇧Tab",
        desc_short: "InsTabs",
        description: "Inspector prev/next tab",
        action: Action::None,
        combos: &[],
    },
    // idx 9: RELOAD
    KeyBinding {
        key_short: "r",
        key: "r",
        desc_short: "Reload",
        description: "Reload metadata",
        action: Action::ReloadMetadata,
        combos: &[KeyCombo::plain(Key::Char('r'))],
    },
    // idx 10: SQL
    KeyBinding {
        key_short: "s",
        key: "s",
        desc_short: "SQL",
        description: "Open SQL Editor",
        action: Action::OpenSqlModal,
        combos: &[KeyCombo::plain(Key::Char('s'))],
    },
    // idx 11: ER_DIAGRAM
    KeyBinding {
        key_short: "e",
        key: "e",
        desc_short: "ER Diagram",
        description: "Open ER Diagram",
        action: Action::OpenErTablePicker,
        combos: &[KeyCombo::plain(Key::Char('e'))],
    },
    // idx 12: CONNECTIONS
    KeyBinding {
        key_short: "c",
        key: "c",
        desc_short: "Connections",
        description: "Open Connection Selector",
        action: Action::OpenConnectionSelector,
        combos: &[KeyCombo::plain(Key::Char('c'))],
    },
    // idx 13: CSV_EXPORT
    KeyBinding {
        key_short: "^E",
        key: "Ctrl+E",
        desc_short: "Export",
        description: "Export result to CSV",
        action: Action::RequestCsvExport,
        combos: &[KeyCombo::ctrl(Key::Char('e'))],
    },
    // idx 14: READ_ONLY / idx 15: EXIT_READ_ONLY — intentionally duplicate combo (Ctrl+R)
    // for the same Action::ToggleReadOnly. Two entries exist because the footer
    // shows different labels depending on whether read-only mode is active.
    KeyBinding {
        key_short: "^R",
        key: "Ctrl+R",
        desc_short: "Read-Only",
        description: "Enable Read-Only mode",
        action: Action::ToggleReadOnly,
        combos: &[KeyCombo::ctrl(Key::Char('r'))],
    },
    KeyBinding {
        key_short: "^R",
        key: "Ctrl+R",
        desc_short: "Read-Write",
        description: "Disable Read-Only mode",
        action: Action::ToggleReadOnly,
        combos: &[KeyCombo::ctrl(Key::Char('r'))],
    },
    // idx 16: QUERY_HISTORY
    KeyBinding {
        key_short: "^O",
        key: "Ctrl+O",
        desc_short: "History",
        description: "Open Query History",
        action: Action::OpenQueryHistoryPicker,
        combos: &[KeyCombo::ctrl(Key::Char('o'))],
    },
];

pub const NAVIGATION_KEYS: &[KeyBinding] = &[
    KeyBinding {
        key_short: "j",
        key: "j / ↓",
        desc_short: "Down",
        description: "Move down / scroll",
        action: Action::None,
        combos: &[],
    },
    KeyBinding {
        key_short: "k",
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

pub const FOOTER_NAV_KEYS: &[KeyBinding] = &[
    // idx 0: SCROLL
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Move down/up",
        action: Action::None,
        combos: &[],
    },
    // idx 1: SCROLL_SHORT (same as SCROLL for now)
    KeyBinding {
        key_short: "j/k / ↑↓",
        key: "j / k / ↑ / ↓",
        desc_short: "Scroll",
        description: "Move down/up",
        action: Action::None,
        combos: &[],
    },
    // idx 2: TOP_BOTTOM
    KeyBinding {
        key_short: "g/G H/M/L",
        key: "g / G / H / M / L",
        desc_short: "Top/Bot/Viewport",
        description: "First/Last item, Viewport top/mid/bot",
        action: Action::None,
        combos: &[],
    },
    // idx 3: H_SCROLL
    KeyBinding {
        key_short: "h/l / ←→",
        key: "h / l / ← / →",
        desc_short: "H-Scroll",
        description: "Scroll left/right",
        action: Action::None,
        combos: &[],
    },
    // idx 4: PAGE_NAV
    KeyBinding {
        key_short: "]/[",
        key: "] / [",
        desc_short: "Page",
        description: "Next/Previous page",
        action: Action::None,
        combos: &[],
    },
];

pub const RESULT_ACTIVE_KEYS: &[KeyBinding] = &[
    // idx 0: ENTER_DEEPEN
    KeyBinding {
        key_short: "Enter",
        key: "Enter",
        desc_short: "Select",
        description: "Enter row / cell selection",
        action: Action::ResultEnterRowActive,
        combos: &[KeyCombo::plain(Key::Enter)],
    },
    // idx 1: YANK
    KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy cell value to clipboard",
        action: Action::ResultCellYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    },
    // idx 2: STAGE_DELETE
    KeyBinding {
        key_short: "dd",
        key: "d, d",
        desc_short: "Stage Del",
        description: "Stage row for deletion (red highlight; :w to commit)",
        action: Action::StageRowForDelete,
        combos: &[], // dd is a two-key sequence, not a single combo
    },
    // idx 3: UNSTAGE_DELETE
    KeyBinding {
        key_short: "u",
        key: "u",
        desc_short: "Unstage",
        description: "Unstage last staged row",
        action: Action::UnstageLastStagedRow,
        combos: &[KeyCombo::plain(Key::Char('u'))],
    },
    // idx 4: CELL_NAV (display-only)
    KeyBinding {
        key_short: "h/l",
        key: "h / l",
        desc_short: "Cell",
        description: "Move cell left/right",
        action: Action::None,
        combos: &[],
    },
    // idx 5: ROW_NAV (display-only)
    KeyBinding {
        key_short: "j/k",
        key: "j / k",
        desc_short: "Row",
        description: "Move row up/down",
        action: Action::None,
        combos: &[],
    },
    // idx 6: TOP_BOTTOM (display-only)
    KeyBinding {
        key_short: "g/G H/M/L",
        key: "g / G / H / M / L",
        desc_short: "Top/Bot/Viewport",
        description: "First/Last row, Viewport top/mid/bot",
        action: Action::None,
        combos: &[],
    },
    // idx 7: ESC_BACK
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Back",
        description: "Exit to previous mode",
        action: Action::ResultExitToScroll,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 8: EDIT
    KeyBinding {
        key_short: "i",
        key: "i",
        desc_short: "Edit",
        description: "Edit active cell",
        action: Action::ResultEnterCellEdit,
        combos: &[KeyCombo::plain(Key::Char('i'))],
    },
    // idx 9: DRAFT_DISCARD
    KeyBinding {
        key_short: "Esc",
        key: "Esc",
        desc_short: "Discard",
        description: "Discard pending draft and exit to Row Active",
        action: Action::ResultDiscardCellEdit,
        combos: &[KeyCombo::plain(Key::Esc)],
    },
    // idx 10: ROW_YANK
    KeyBinding {
        key_short: "yy",
        key: "y, y",
        desc_short: "Yank Row",
        description: "Copy row values to clipboard (TSV)",
        action: Action::ResultRowYank,
        combos: &[],
    },
];

pub const HISTORY_KEYS: &[KeyBinding] = &[
    // idx 0: OPEN
    KeyBinding {
        key_short: "^H",
        key: "Ctrl+H",
        desc_short: "History",
        description: "Toggle Result History",
        action: Action::OpenResultHistory,
        combos: &[KeyCombo::ctrl(Key::Char('h'))],
    },
    // idx 1: NAV (display-only)
    KeyBinding {
        key_short: "]/[",
        key: "] / [",
        desc_short: "History",
        description: "Navigate history newer/older",
        action: Action::None,
        combos: &[],
    },
    // idx 2: EXIT (display-only)
    KeyBinding {
        key_short: "^H",
        key: "Ctrl+H",
        desc_short: "Back",
        description: "Exit history (back to latest)",
        action: Action::None,
        combos: &[],
    },
];

pub const INSPECTOR_DDL_KEYS: &[KeyBinding] = &[
    // idx 0: YANK
    KeyBinding {
        key_short: "y",
        key: "y",
        desc_short: "Yank",
        description: "Copy DDL to clipboard",
        action: Action::DdlYank,
        combos: &[KeyCombo::plain(Key::Char('y'))],
    },
];
