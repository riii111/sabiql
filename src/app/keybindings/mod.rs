//! Centralized keybinding definitions.
//! Single source of truth for key/description used by Footer, Help, and Palette.

mod connections;
mod editors;
mod normal;
mod overlays;
pub mod types;

use crate::app::action::Action;
pub use connections::*;
pub use editors::*;
pub use normal::*;
pub use overlays::*;
pub use types::{Key, KeyCombo, Modifiers};

// =============================================================================
// KeyBinding
// =============================================================================

#[derive(Clone)]
pub struct KeyBinding {
    /// Short key for Footer (e.g., "^P", "j/k")
    pub key_short: &'static str,
    /// Display key for Help/Palette (e.g., "Ctrl+P", "j / ↓")
    pub key: &'static str,
    /// Short description for Footer (e.g., "Quit", "Scroll")
    pub desc_short: &'static str,
    /// Full description for Help/Palette (e.g., "Quit application")
    pub description: &'static str,
    /// The action triggered by this key.
    ///
    /// `Action::None` means **display-only**: the entry is shown in Footer/Help/Palette
    /// as a hint but is not matched by `handler.rs`. This is used for multi-key
    /// combined display (e.g., `"j/k / ↑↓"`) or navigation descriptions where the
    /// actual matching is handled directly in handler match arms.
    pub action: Action,
    /// The key combinations that trigger this binding.
    ///
    /// `Action::None` entries always have `combos: &[]` — display text comes
    /// from `key_short`/`key` strings. Exec-only bindings that don't appear
    /// in Footer/Help live in separate `*_HIDDEN` constants.
    pub combos: &'static [KeyCombo],
}

impl KeyBinding {
    /// Returns (key_short, desc_short) tuple for Footer display
    pub const fn as_hint(&self) -> (&'static str, &'static str) {
        (self.key_short, self.desc_short)
    }
}

/// A mode's keybinding set: display entries (for Footer/Help) paired with
/// hidden exec-only entries (for `keymap::resolve()`).
///
/// By bundling both slices into one value we structurally guarantee that
/// every mode with hidden bindings also declares its display counterpart,
/// and vice-versa.
pub struct ModeBindings {
    pub display: &'static [KeyBinding],
    pub hidden: &'static [KeyBinding],
}

impl ModeBindings {
    /// Resolve a combo against display entries first, then hidden entries.
    pub fn resolve(&self, combo: &KeyCombo) -> Option<Action> {
        crate::app::keymap::resolve(combo, self.display)
            .or_else(|| crate::app::keymap::resolve(combo, self.hidden))
    }
}

pub const HELP: ModeBindings = ModeBindings {
    display: HELP_KEYS,
    hidden: HELP_HIDDEN,
};
pub const CONNECTION_ERROR: ModeBindings = ModeBindings {
    display: CONNECTION_ERROR_KEYS,
    hidden: CONNECTION_ERROR_HIDDEN,
};
pub const TABLE_PICKER: ModeBindings = ModeBindings {
    display: TABLE_PICKER_KEYS,
    hidden: TABLE_PICKER_HIDDEN,
};
pub const ER_PICKER: ModeBindings = ModeBindings {
    display: ER_PICKER_KEYS,
    hidden: ER_PICKER_HIDDEN,
};
pub const COMMAND_PALETTE: ModeBindings = ModeBindings {
    display: COMMAND_PALETTE_KEYS,
    hidden: COMMAND_PALETTE_HIDDEN,
};
pub const CONNECTION_SELECTOR: ModeBindings = ModeBindings {
    display: CONNECTION_SELECTOR_KEYS,
    hidden: CONNECTION_SELECTOR_HIDDEN,
};

/// All modes that use display + hidden split.
/// Used by semantic tests to exhaustively validate structural invariants.
pub const ALL_MODE_BINDINGS: &[(&str, &ModeBindings)] = &[
    ("HELP", &HELP),
    ("CONNECTION_ERROR", &CONNECTION_ERROR),
    ("TABLE_PICKER", &TABLE_PICKER),
    ("ER_PICKER", &ER_PICKER),
    ("COMMAND_PALETTE", &COMMAND_PALETTE),
    ("CONNECTION_SELECTOR", &CONNECTION_SELECTOR),
];

// =============================================================================
// Index Constants for Footer Lookup
// =============================================================================

pub mod idx {
    pub mod global {
        pub const QUIT: usize = 0;
        pub const HELP: usize = 1;
        pub const TABLE_PICKER: usize = 2;
        pub const PALETTE: usize = 3;
        pub const COMMAND_LINE: usize = 4;
        pub const FOCUS: usize = 5;
        pub const EXIT_FOCUS: usize = 6;
        pub const PANE_SWITCH: usize = 7;
        pub const INSPECTOR_TABS: usize = 8;
        pub const RELOAD: usize = 9;
        pub const SQL: usize = 10;
        pub const ER_DIAGRAM: usize = 11;
        pub const CONNECTIONS: usize = 12;
    }

    /// Indexes for FOOTER_NAV_KEYS
    pub mod footer_nav {
        pub const SCROLL: usize = 0;
        pub const SCROLL_SHORT: usize = 1;
        pub const TOP_BOTTOM: usize = 2;
        pub const H_SCROLL: usize = 3;
        pub const PAGE_NAV: usize = 4;
    }

    pub mod sql_modal {
        pub const RUN: usize = 0;
        pub const ESC_CLOSE: usize = 1;
        pub const MOVE: usize = 2;
        pub const HOME_END: usize = 3;
        pub const TAB: usize = 4;
        pub const COMPLETION_TRIGGER: usize = 5;
        pub const CLEAR: usize = 6;
    }

    pub mod overlay {
        pub const ESC_CANCEL: usize = 0;
        pub const ESC_CLOSE: usize = 1;
        pub const ENTER_EXECUTE: usize = 2;
        pub const ENTER_SELECT: usize = 3;
        pub const NAVIGATE_JK: usize = 4;
        pub const TYPE_FILTER: usize = 5;
        pub const ERROR_OPEN: usize = 6;
    }

    pub mod conn_setup {
        pub const TAB_NAV: usize = 0;
        pub const TAB_NEXT: usize = 1;
        pub const TAB_PREV: usize = 2;
        pub const SAVE: usize = 3;
        pub const ESC_CANCEL: usize = 4;
        pub const ENTER_DROPDOWN: usize = 5;
        pub const DROPDOWN_NAV: usize = 6;
    }

    pub mod conn_error {
        pub const EDIT: usize = 0;
        pub const SWITCH: usize = 1;
        pub const DETAILS: usize = 2;
        pub const COPY: usize = 3;
        pub const SCROLL: usize = 4;
        pub const ESC_CLOSE: usize = 5;
        pub const QUIT: usize = 6;
    }

    pub mod confirm {
        pub const YES: usize = 0;
        pub const NO: usize = 1;
    }

    pub mod table_picker {
        pub const ENTER_SELECT: usize = 0;
        pub const NAVIGATE: usize = 1;
        pub const TYPE_FILTER: usize = 2;
        pub const ESC_CLOSE: usize = 3;
    }

    pub mod er_picker {
        pub const ENTER_GENERATE: usize = 0;
        pub const SELECT: usize = 1;
        pub const SELECT_ALL: usize = 2;
        pub const NAVIGATE: usize = 3;
        pub const TYPE_FILTER: usize = 4;
        pub const ESC_CLOSE: usize = 5;
    }

    pub mod cmd_palette {
        pub const ENTER_EXECUTE: usize = 0;
        pub const NAVIGATE_JK: usize = 1;
        pub const ESC_CLOSE: usize = 2;
    }

    pub mod help {
        pub const SCROLL: usize = 0;
        pub const CLOSE: usize = 1;
        pub const QUIT: usize = 2;
    }

    pub mod result_active {
        pub const ENTER_DEEPEN: usize = 0;
        pub const YANK: usize = 1;
        pub const STAGE_DELETE: usize = 2;
        pub const UNSTAGE_DELETE: usize = 3;
        pub const CELL_NAV: usize = 4;
        pub const ROW_NAV: usize = 5;
        pub const TOP_BOTTOM: usize = 6;
        pub const ESC_BACK: usize = 7;
        pub const EDIT: usize = 8;
        pub const DRAFT_DISCARD: usize = 9;
    }

    pub mod cell_edit {
        pub const WRITE: usize = 0;
        pub const TYPE: usize = 1;
        pub const COMMAND: usize = 2;
        pub const ESC_CANCEL: usize = 3;
    }

    pub mod connections_mode {
        pub const CONNECT: usize = 0;
        pub const NEW: usize = 1;
        pub const EDIT: usize = 2;
        pub const DELETE: usize = 3;
        pub const NAVIGATE: usize = 4;
        pub const HELP: usize = 5;
        pub const TABLES: usize = 6;
        pub const BACK: usize = 7;
        pub const QUIT: usize = 8;
    }

    pub mod connection_selector {
        pub const CONFIRM: usize = 0;
        pub const SELECT: usize = 1;
        pub const NEW: usize = 2;
        pub const EDIT: usize = 3;
        pub const DELETE: usize = 4;
        pub const QUIT: usize = 5;
    }
}

// =============================================================================
// Help Overlay Layout
// =============================================================================

/// Returns total line count of help overlay content.
///
/// Derived from the same section order as `HelpOverlay::render()`:
/// each section = 1 header + N key lines, separated by 1 blank line.
pub const fn help_content_line_count() -> usize {
    // 16 sections × 1 header each = 16
    // 15 blank-line separators between sections = 15
    16 + 15
        + GLOBAL_KEYS.len()
        + NAVIGATION_KEYS.len()
        + RESULT_ACTIVE_KEYS.len()
        + CELL_EDIT_KEYS.len()
        + SQL_MODAL_KEYS.len()
        + OVERLAY_KEYS.len()
        + COMMAND_LINE_KEYS.len()
        + CONNECTION_SETUP_KEYS.len()
        + CONNECTION_ERROR_KEYS.len()
        + CONNECTIONS_MODE_KEYS.len()
        + CONNECTION_SELECTOR_KEYS.len()
        + ER_PICKER_KEYS.len()
        + TABLE_PICKER_KEYS.len()
        + COMMAND_PALETTE_KEYS.len()
        + HELP_KEYS.len()
        + CONFIRM_DIALOG_KEYS.len()
}

// =============================================================================
// Predicate functions for Normal mode routing
// =============================================================================

pub fn is_quit(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::QUIT].combos.contains(combo)
}

pub fn is_help(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::HELP].combos.contains(combo)
}

pub fn is_table_picker(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::TABLE_PICKER]
        .combos
        .contains(combo)
}

pub fn is_command_palette(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::PALETTE].combos.contains(combo)
}

pub fn is_command_line(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::COMMAND_LINE]
        .combos
        .contains(combo)
}

pub fn is_focus_toggle(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::FOCUS].combos.contains(combo)
}

pub fn is_reload(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::RELOAD].combos.contains(combo)
}

pub fn is_open_sql(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::SQL].combos.contains(combo)
}

pub fn is_open_er(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::ER_DIAGRAM].combos.contains(combo)
}

pub fn is_toggle_connections(combo: &KeyCombo) -> bool {
    GLOBAL_KEYS[idx::global::CONNECTIONS].combos.contains(combo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idx_constants_are_within_bounds() {
        // GLOBAL_KEYS
        assert!(idx::global::QUIT < GLOBAL_KEYS.len());
        assert!(idx::global::HELP < GLOBAL_KEYS.len());
        assert!(idx::global::TABLE_PICKER < GLOBAL_KEYS.len());
        assert!(idx::global::PALETTE < GLOBAL_KEYS.len());
        assert!(idx::global::COMMAND_LINE < GLOBAL_KEYS.len());
        assert!(idx::global::FOCUS < GLOBAL_KEYS.len());
        assert!(idx::global::EXIT_FOCUS < GLOBAL_KEYS.len());
        assert!(idx::global::PANE_SWITCH < GLOBAL_KEYS.len());
        assert!(idx::global::INSPECTOR_TABS < GLOBAL_KEYS.len());
        assert!(idx::global::RELOAD < GLOBAL_KEYS.len());
        assert!(idx::global::SQL < GLOBAL_KEYS.len());
        assert!(idx::global::ER_DIAGRAM < GLOBAL_KEYS.len());
        assert!(idx::global::CONNECTIONS < GLOBAL_KEYS.len());

        // FOOTER_NAV_KEYS
        assert!(idx::footer_nav::SCROLL < FOOTER_NAV_KEYS.len());
        assert!(idx::footer_nav::SCROLL_SHORT < FOOTER_NAV_KEYS.len());
        assert!(idx::footer_nav::TOP_BOTTOM < FOOTER_NAV_KEYS.len());
        assert!(idx::footer_nav::H_SCROLL < FOOTER_NAV_KEYS.len());
        assert!(idx::footer_nav::PAGE_NAV < FOOTER_NAV_KEYS.len());

        // SQL_MODAL_KEYS
        assert!(idx::sql_modal::RUN < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::ESC_CLOSE < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::MOVE < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::HOME_END < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::TAB < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::COMPLETION_TRIGGER < SQL_MODAL_KEYS.len());
        assert!(idx::sql_modal::CLEAR < SQL_MODAL_KEYS.len());

        // OVERLAY_KEYS
        assert!(idx::overlay::ESC_CANCEL < OVERLAY_KEYS.len());
        assert!(idx::overlay::ESC_CLOSE < OVERLAY_KEYS.len());
        assert!(idx::overlay::ENTER_EXECUTE < OVERLAY_KEYS.len());
        assert!(idx::overlay::ENTER_SELECT < OVERLAY_KEYS.len());
        assert!(idx::overlay::NAVIGATE_JK < OVERLAY_KEYS.len());
        assert!(idx::overlay::TYPE_FILTER < OVERLAY_KEYS.len());
        assert!(idx::overlay::ERROR_OPEN < OVERLAY_KEYS.len());

        // CONNECTION_SETUP_KEYS
        assert!(idx::conn_setup::TAB_NAV < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::TAB_NEXT < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::TAB_PREV < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::SAVE < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::ESC_CANCEL < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::ENTER_DROPDOWN < CONNECTION_SETUP_KEYS.len());
        assert!(idx::conn_setup::DROPDOWN_NAV < CONNECTION_SETUP_KEYS.len());

        // CONNECTION_ERROR_KEYS
        assert!(idx::conn_error::EDIT < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::DETAILS < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::COPY < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::SCROLL < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::ESC_CLOSE < CONNECTION_ERROR_KEYS.len());
        assert!(idx::conn_error::QUIT < CONNECTION_ERROR_KEYS.len());

        // CONFIRM_DIALOG_KEYS
        assert!(idx::confirm::YES < CONFIRM_DIALOG_KEYS.len());
        assert!(idx::confirm::NO < CONFIRM_DIALOG_KEYS.len());

        // TABLE_PICKER_KEYS
        assert!(idx::table_picker::ENTER_SELECT < TABLE_PICKER_KEYS.len());
        assert!(idx::table_picker::NAVIGATE < TABLE_PICKER_KEYS.len());
        assert!(idx::table_picker::TYPE_FILTER < TABLE_PICKER_KEYS.len());
        assert!(idx::table_picker::ESC_CLOSE < TABLE_PICKER_KEYS.len());

        // ER_PICKER_KEYS
        assert!(idx::er_picker::ENTER_GENERATE < ER_PICKER_KEYS.len());
        assert!(idx::er_picker::NAVIGATE < ER_PICKER_KEYS.len());
        assert!(idx::er_picker::TYPE_FILTER < ER_PICKER_KEYS.len());
        assert!(idx::er_picker::ESC_CLOSE < ER_PICKER_KEYS.len());

        // COMMAND_PALETTE_KEYS
        assert!(idx::cmd_palette::ENTER_EXECUTE < COMMAND_PALETTE_KEYS.len());
        assert!(idx::cmd_palette::NAVIGATE_JK < COMMAND_PALETTE_KEYS.len());
        assert!(idx::cmd_palette::ESC_CLOSE < COMMAND_PALETTE_KEYS.len());

        // HELP_KEYS
        assert!(idx::help::SCROLL < HELP_KEYS.len());
        assert!(idx::help::CLOSE < HELP_KEYS.len());
        assert!(idx::help::QUIT < HELP_KEYS.len());

        // RESULT_ACTIVE_KEYS
        assert!(idx::result_active::ENTER_DEEPEN < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::YANK < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::STAGE_DELETE < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::UNSTAGE_DELETE < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::CELL_NAV < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::ROW_NAV < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::TOP_BOTTOM < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::ESC_BACK < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::EDIT < RESULT_ACTIVE_KEYS.len());
        assert!(idx::result_active::DRAFT_DISCARD < RESULT_ACTIVE_KEYS.len());

        // CELL_EDIT_KEYS
        assert!(idx::cell_edit::WRITE < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::TYPE < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::COMMAND < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::ESC_CANCEL < CELL_EDIT_KEYS.len());

        // CONNECTIONS_MODE_KEYS
        assert!(idx::connections_mode::CONNECT < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::NEW < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::EDIT < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::DELETE < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::NAVIGATE < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::HELP < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::TABLES < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::BACK < CONNECTIONS_MODE_KEYS.len());
        assert!(idx::connections_mode::QUIT < CONNECTIONS_MODE_KEYS.len());

        // CONNECTION_SELECTOR_KEYS
        assert!(idx::connection_selector::CONFIRM < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::SELECT < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::NEW < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::EDIT < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::DELETE < CONNECTION_SELECTOR_KEYS.len());
        assert!(idx::connection_selector::QUIT < CONNECTION_SELECTOR_KEYS.len());
    }

    #[test]
    fn help_content_line_count_matches_section_structure() {
        let sections: &[usize] = &[
            GLOBAL_KEYS.len(),
            NAVIGATION_KEYS.len(),
            RESULT_ACTIVE_KEYS.len(),
            CELL_EDIT_KEYS.len(),
            SQL_MODAL_KEYS.len(),
            OVERLAY_KEYS.len(),
            COMMAND_LINE_KEYS.len(),
            CONNECTION_SETUP_KEYS.len(),
            CONNECTION_ERROR_KEYS.len(),
            CONNECTIONS_MODE_KEYS.len(),
            CONNECTION_SELECTOR_KEYS.len(),
            ER_PICKER_KEYS.len(),
            TABLE_PICKER_KEYS.len(),
            COMMAND_PALETTE_KEYS.len(),
            HELP_KEYS.len(),
            CONFIRM_DIALOG_KEYS.len(),
        ];
        let section_count = sections.len();
        let expected: usize = section_count + sections.iter().sum::<usize>() + (section_count - 1);

        assert_eq!(help_content_line_count(), expected);
    }

    /// Semantic consistency tests (#126)
    mod semantic {
        use super::*;
        use crate::app::keymap;
        use rstest::rstest;

        // ------------------------------------------------------------------ //
        // 1. idx-to-Action correctness
        // ------------------------------------------------------------------ //

        #[rstest]
        #[case(idx::global::QUIT, Action::Quit)]
        #[case(idx::global::HELP, Action::OpenHelp)]
        #[case(idx::global::TABLE_PICKER, Action::OpenTablePicker)]
        #[case(idx::global::PALETTE, Action::OpenCommandPalette)]
        #[case(idx::global::COMMAND_LINE, Action::EnterCommandLine)]
        #[case(idx::global::RELOAD, Action::ReloadMetadata)]
        #[case(idx::global::SQL, Action::OpenSqlModal)]
        #[case(idx::global::ER_DIAGRAM, Action::OpenErTablePicker)]
        #[case(idx::global::CONNECTIONS, Action::ToggleExplorerMode)]
        fn global_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&GLOBAL_KEYS[i].action) == std::mem::discriminant(&expected),
                "GLOBAL_KEYS[{i}] has action {:?}, expected {expected:?}",
                GLOBAL_KEYS[i].action
            );
        }

        #[rstest]
        #[case(idx::help::CLOSE, Action::CloseHelp)]
        #[case(idx::help::QUIT, Action::Quit)]
        fn help_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&HELP_KEYS[i].action) == std::mem::discriminant(&expected),
                "HELP_KEYS[{i}] has action {:?}, expected {expected:?}",
                HELP_KEYS[i].action
            );
        }

        #[rstest]
        #[case(idx::confirm::YES, Action::ConfirmDialogConfirm)]
        #[case(idx::confirm::NO, Action::ConfirmDialogCancel)]
        fn confirm_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&CONFIRM_DIALOG_KEYS[i].action)
                    == std::mem::discriminant(&expected),
                "CONFIRM_DIALOG_KEYS[{i}] has action {:?}, expected {expected:?}",
                CONFIRM_DIALOG_KEYS[i].action
            );
        }

        #[rstest]
        #[case(idx::conn_error::QUIT, Action::Quit)]
        #[case(idx::conn_error::ESC_CLOSE, Action::CloseConnectionError)]
        #[case(idx::conn_error::EDIT, Action::ReenterConnectionSetup)]
        #[case(idx::conn_error::SWITCH, Action::OpenConnectionSelector)]
        #[case(idx::conn_error::DETAILS, Action::ToggleConnectionErrorDetails)]
        #[case(idx::conn_error::COPY, Action::CopyConnectionError)]
        fn conn_error_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&CONNECTION_ERROR_KEYS[i].action)
                    == std::mem::discriminant(&expected),
                "CONNECTION_ERROR_KEYS[{i}] has action {:?}, expected {expected:?}",
                CONNECTION_ERROR_KEYS[i].action
            );
        }

        #[rstest]
        #[case(idx::connection_selector::CONFIRM, Action::ConfirmConnectionSelection)]
        #[case(idx::connection_selector::NEW, Action::OpenConnectionSetup)]
        #[case(idx::connection_selector::EDIT, Action::RequestEditSelectedConnection)]
        #[case(
            idx::connection_selector::DELETE,
            Action::RequestDeleteSelectedConnection
        )]
        #[case(idx::connection_selector::QUIT, Action::Quit)]
        fn connection_selector_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&CONNECTION_SELECTOR_KEYS[i].action)
                    == std::mem::discriminant(&expected),
                "CONNECTION_SELECTOR_KEYS[{i}] has action {:?}, expected {expected:?}",
                CONNECTION_SELECTOR_KEYS[i].action
            );
        }

        // ------------------------------------------------------------------ //
        // 2. Non-None bindings have at least one combo
        // ------------------------------------------------------------------ //

        fn check_non_none_have_combos(bindings: &[KeyBinding], name: &str) {
            for (i, kb) in bindings.iter().enumerate() {
                if !matches!(kb.action, Action::None) && kb.combos.is_empty() {
                    if kb.key.starts_with(':') {
                        continue;
                    }
                    if kb.key_short == ":w" || kb.desc_short == "Write" {
                        continue;
                    }
                    panic!(
                        "{name}[{i}] has action {:?} but no combos (key={:?})",
                        kb.action, kb.key
                    );
                }
            }
        }

        #[test]
        fn all_non_none_bindings_have_combos() {
            check_non_none_have_combos(GLOBAL_KEYS, "GLOBAL_KEYS");
            check_non_none_have_combos(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
            check_non_none_have_combos(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
            check_non_none_have_combos(CELL_EDIT_KEYS, "CELL_EDIT_KEYS");
            for (name, mb) in ALL_MODE_BINDINGS {
                check_non_none_have_combos(mb.display, &format!("{name} display"));
                check_non_none_have_combos(mb.hidden, &format!("{name} hidden"));
            }
        }

        // ------------------------------------------------------------------ //
        // 3. No duplicate combos within simple (non-context-dependent) modes
        // ------------------------------------------------------------------ //

        fn check_no_duplicate_combos(bindings: &[KeyBinding], name: &str) {
            let mut seen: Vec<KeyCombo> = Vec::new();
            for kb in bindings
                .iter()
                .filter(|kb| !matches!(kb.action, Action::None))
            {
                for combo in kb.combos {
                    if seen.contains(combo) {
                        panic!(
                            "{name}: duplicate combo {combo:?} in binding {:?}",
                            kb.action
                        );
                    }
                    seen.push(*combo);
                }
            }
        }

        fn check_no_duplicate_combos_combined(
            main: &[KeyBinding],
            hidden: &[KeyBinding],
            name: &str,
        ) {
            let combined: Vec<_> = main.iter().chain(hidden.iter()).cloned().collect();
            check_no_duplicate_combos(&combined, name);
        }

        /// GLOBAL_KEYS excluded: idx 5/6 (FOCUS/EXIT_FOCUS) intentionally share
        /// the same combo for different footer labels. This is a display concern
        /// that should be refactored into footer logic, not keybinding data.
        #[test]
        fn no_duplicate_combos_in_simple_modes() {
            check_no_duplicate_combos(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
            check_no_duplicate_combos(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
            for (name, mb) in ALL_MODE_BINDINGS {
                check_no_duplicate_combos_combined(mb.display, mb.hidden, name);
            }
        }

        // ------------------------------------------------------------------ //
        // 4. keymap::resolve() round-trip
        // ------------------------------------------------------------------ //

        fn check_keymap_roundtrip(bindings: &[KeyBinding], name: &str) {
            for kb in bindings
                .iter()
                .filter(|kb| !matches!(kb.action, Action::None))
            {
                for combo in kb.combos {
                    let resolved = keymap::resolve(combo, bindings);
                    match resolved {
                        Some(ref action)
                            if std::mem::discriminant(action)
                                == std::mem::discriminant(&kb.action) => {}
                        other => panic!(
                            "{name}: combo {combo:?} resolved to {other:?}, expected {:?}",
                            kb.action
                        ),
                    }
                }
            }
        }

        #[test]
        fn keymap_resolve_roundtrip_for_simple_modes() {
            check_keymap_roundtrip(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
            check_keymap_roundtrip(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
            for (name, mb) in ALL_MODE_BINDINGS {
                check_keymap_roundtrip(mb.display, &format!("{name} display"));
                check_keymap_roundtrip(mb.hidden, &format!("{name} hidden"));
            }
        }

        // ------------------------------------------------------------------ //
        // 5. Char fallback safety
        // ------------------------------------------------------------------ //

        fn check_no_plain_char_in_filter_mode(
            bindings: &[KeyBinding],
            name: &str,
            allowed_chars: &[char],
        ) {
            let no_mods = Modifiers {
                ctrl: false,
                alt: false,
                shift: false,
            };
            for kb in bindings
                .iter()
                .filter(|kb| !matches!(kb.action, Action::None))
            {
                for combo in kb.combos {
                    if combo.modifiers == no_mods
                        && let Key::Char(c) = combo.key
                    {
                        assert!(
                            allowed_chars.contains(&c),
                            "{name}: executable entry {:?} has plain Char({c:?}) combo \
                             which would shadow filter input",
                            kb.action
                        );
                    }
                }
            }
        }

        #[test]
        fn table_picker_has_no_plain_char_combos() {
            check_no_plain_char_in_filter_mode(TABLE_PICKER_KEYS, "TABLE_PICKER_KEYS", &[]);
            check_no_plain_char_in_filter_mode(TABLE_PICKER_HIDDEN, "TABLE_PICKER_HIDDEN", &[]);
        }

        #[test]
        fn er_picker_has_no_plain_char_combos() {
            check_no_plain_char_in_filter_mode(ER_PICKER_KEYS, "ER_PICKER_KEYS", &[' ']);
            check_no_plain_char_in_filter_mode(ER_PICKER_HIDDEN, "ER_PICKER_HIDDEN", &[' ']);
        }

        #[test]
        fn command_line_has_no_problematic_plain_char_combos() {
            check_no_plain_char_in_filter_mode(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS", &[]);
        }

        #[test]
        fn cell_edit_plain_char_combos_are_intentional() {
            check_no_plain_char_in_filter_mode(CELL_EDIT_KEYS, "CELL_EDIT_KEYS", &[':']);
        }

        // ------------------------------------------------------------------ //
        // 6. Action::None entries must have combos: &[]
        // ------------------------------------------------------------------ //

        fn check_none_action_entries_have_no_combos(bindings: &[KeyBinding], name: &str) {
            for (i, kb) in bindings.iter().enumerate() {
                if matches!(kb.action, Action::None) && !kb.combos.is_empty() {
                    panic!(
                        "{name}[{i}] has action Action::None but non-empty combos: {:?}",
                        kb.combos
                    );
                }
            }
        }

        #[test]
        fn none_action_entries_have_no_combos() {
            check_none_action_entries_have_no_combos(GLOBAL_KEYS, "GLOBAL_KEYS");
            check_none_action_entries_have_no_combos(NAVIGATION_KEYS, "NAVIGATION_KEYS");
            check_none_action_entries_have_no_combos(FOOTER_NAV_KEYS, "FOOTER_NAV_KEYS");
            check_none_action_entries_have_no_combos(SQL_MODAL_KEYS, "SQL_MODAL_KEYS");
            check_none_action_entries_have_no_combos(OVERLAY_KEYS, "OVERLAY_KEYS");
            check_none_action_entries_have_no_combos(
                CONNECTION_SETUP_KEYS,
                "CONNECTION_SETUP_KEYS",
            );
            check_none_action_entries_have_no_combos(RESULT_ACTIVE_KEYS, "RESULT_ACTIVE_KEYS");
            check_none_action_entries_have_no_combos(
                CONNECTIONS_MODE_KEYS,
                "CONNECTIONS_MODE_KEYS",
            );
            check_none_action_entries_have_no_combos(HELP_KEYS, "HELP_KEYS");
            check_none_action_entries_have_no_combos(
                CONNECTION_ERROR_KEYS,
                "CONNECTION_ERROR_KEYS",
            );
            check_none_action_entries_have_no_combos(TABLE_PICKER_KEYS, "TABLE_PICKER_KEYS");
            check_none_action_entries_have_no_combos(ER_PICKER_KEYS, "ER_PICKER_KEYS");
            check_none_action_entries_have_no_combos(COMMAND_PALETTE_KEYS, "COMMAND_PALETTE_KEYS");
            check_none_action_entries_have_no_combos(
                CONNECTION_SELECTOR_KEYS,
                "CONNECTION_SELECTOR_KEYS",
            );
        }

        // ------------------------------------------------------------------ //
        // 7. Hidden arrays: every entry must have a real action and combos
        // ------------------------------------------------------------------ //

        fn check_hidden_entries_valid(bindings: &[KeyBinding], name: &str) {
            for (i, kb) in bindings.iter().enumerate() {
                assert!(
                    !matches!(kb.action, Action::None),
                    "{name}[{i}] is in a HIDDEN array but has Action::None"
                );
                assert!(
                    !kb.combos.is_empty(),
                    "{name}[{i}] is in a HIDDEN array but has no combos"
                );
            }
        }

        #[test]
        fn hidden_exec_entries_are_valid() {
            for (name, mb) in ALL_MODE_BINDINGS {
                check_hidden_entries_valid(mb.hidden, &format!("{name} hidden"));
            }
        }
    }
}
