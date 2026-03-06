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
    pub key_short: &'static str,
    pub key: &'static str,
    pub desc_short: &'static str,
    pub description: &'static str,
    /// `Action::None` = display-only (footer/help hint, not resolved by keymap).
    pub action: Action,
    /// Empty for `Action::None` entries; display text comes from `key_short`/`key`.
    pub combos: &'static [KeyCombo],
}

impl KeyBinding {
    pub const fn as_hint(&self) -> (&'static str, &'static str) {
        (self.key_short, self.desc_short)
    }
}

// =============================================================================
// ModeRow — unified single-definition model for mixed modes
// =============================================================================

pub struct ExecBinding {
    pub action: Action,
    pub combos: &'static [KeyCombo],
}

pub struct ModeRow {
    pub key_short: &'static str,
    pub key: &'static str,
    pub desc_short: &'static str,
    pub description: &'static str,
    pub bindings: &'static [ExecBinding],
}

impl ModeRow {
    pub const fn as_hint(&self) -> (&'static str, &'static str) {
        (self.key_short, self.desc_short)
    }
}

/// Unified mode bindings backed by `ModeRow` slices.
pub struct ModeBindings {
    pub rows: &'static [ModeRow],
}

impl ModeBindings {
    pub fn resolve(&self, combo: &KeyCombo) -> Option<Action> {
        crate::app::keymap::resolve_mode(combo, self.rows)
    }
}

pub const HELP: ModeBindings = ModeBindings { rows: HELP_ROWS };
pub const CONNECTION_ERROR: ModeBindings = ModeBindings {
    rows: CONNECTION_ERROR_ROWS,
};
pub const TABLE_PICKER: ModeBindings = ModeBindings {
    rows: TABLE_PICKER_ROWS,
};
pub const ER_PICKER: ModeBindings = ModeBindings {
    rows: ER_PICKER_ROWS,
};
pub const COMMAND_PALETTE: ModeBindings = ModeBindings {
    rows: COMMAND_PALETTE_ROWS,
};
pub const CONNECTION_SELECTOR: ModeBindings = ModeBindings {
    rows: CONNECTION_SELECTOR_ROWS,
};

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
        pub const RETRY: usize = 6;
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
        pub const MOVE: usize = 2;
        pub const HOME_END: usize = 3;
        pub const COMMAND: usize = 4;
        pub const ESC_CANCEL: usize = 5;
    }

    pub mod connection_selector {
        pub const CONFIRM: usize = 0;
        pub const SELECT: usize = 1;
        pub const NEW: usize = 2;
        pub const EDIT: usize = 3;
        pub const DELETE: usize = 4;
        pub const CLOSE: usize = 5;
    }

    pub mod inspector_ddl {
        pub const YANK: usize = 0;
    }

    pub mod history {
        pub const OPEN: usize = 0;
        pub const NAV: usize = 1;
        pub const EXIT: usize = 2;
    }
}

// =============================================================================
// Help Overlay Layout
// =============================================================================

/// Must match the section order in `HelpOverlay::render()`.
pub const fn help_content_line_count() -> usize {
    // 17 sections × 1 header each = 17
    // 16 blank-line separators between sections = 16
    17 + 16
        + GLOBAL_KEYS.len()
        + NAVIGATION_KEYS.len()
        + HISTORY_KEYS.len()
        + RESULT_ACTIVE_KEYS.len()
        + INSPECTOR_DDL_KEYS.len()
        + CELL_EDIT_KEYS.len()
        + SQL_MODAL_KEYS.len()
        + OVERLAY_KEYS.len()
        + COMMAND_LINE_KEYS.len()
        + CONNECTION_SETUP_KEYS.len()
        + CONNECTION_ERROR_ROWS.len()
        + CONNECTION_SELECTOR_ROWS.len()
        + ER_PICKER_ROWS.len()
        + TABLE_PICKER_ROWS.len()
        + COMMAND_PALETTE_ROWS.len()
        + HELP_ROWS.len()
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

        // CONNECTION_ERROR_ROWS
        assert!(idx::conn_error::EDIT < CONNECTION_ERROR_ROWS.len());
        assert!(idx::conn_error::SWITCH < CONNECTION_ERROR_ROWS.len());
        assert!(idx::conn_error::DETAILS < CONNECTION_ERROR_ROWS.len());
        assert!(idx::conn_error::COPY < CONNECTION_ERROR_ROWS.len());
        assert!(idx::conn_error::SCROLL < CONNECTION_ERROR_ROWS.len());
        assert!(idx::conn_error::ESC_CLOSE < CONNECTION_ERROR_ROWS.len());

        // CONFIRM_DIALOG_KEYS
        assert!(idx::confirm::YES < CONFIRM_DIALOG_KEYS.len());
        assert!(idx::confirm::NO < CONFIRM_DIALOG_KEYS.len());

        // TABLE_PICKER_ROWS
        assert!(idx::table_picker::ENTER_SELECT < TABLE_PICKER_ROWS.len());
        assert!(idx::table_picker::NAVIGATE < TABLE_PICKER_ROWS.len());
        assert!(idx::table_picker::TYPE_FILTER < TABLE_PICKER_ROWS.len());
        assert!(idx::table_picker::ESC_CLOSE < TABLE_PICKER_ROWS.len());

        // ER_PICKER_ROWS
        assert!(idx::er_picker::ENTER_GENERATE < ER_PICKER_ROWS.len());
        assert!(idx::er_picker::SELECT < ER_PICKER_ROWS.len());
        assert!(idx::er_picker::SELECT_ALL < ER_PICKER_ROWS.len());
        assert!(idx::er_picker::NAVIGATE < ER_PICKER_ROWS.len());
        assert!(idx::er_picker::TYPE_FILTER < ER_PICKER_ROWS.len());
        assert!(idx::er_picker::ESC_CLOSE < ER_PICKER_ROWS.len());

        // COMMAND_PALETTE_ROWS
        assert!(idx::cmd_palette::ENTER_EXECUTE < COMMAND_PALETTE_ROWS.len());
        assert!(idx::cmd_palette::NAVIGATE_JK < COMMAND_PALETTE_ROWS.len());
        assert!(idx::cmd_palette::ESC_CLOSE < COMMAND_PALETTE_ROWS.len());

        // HELP_ROWS
        assert!(idx::help::SCROLL < HELP_ROWS.len());
        assert!(idx::help::CLOSE < HELP_ROWS.len());

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

        // HISTORY_KEYS
        assert!(idx::history::OPEN < HISTORY_KEYS.len());
        assert!(idx::history::NAV < HISTORY_KEYS.len());
        assert!(idx::history::EXIT < HISTORY_KEYS.len());

        // INSPECTOR_DDL_KEYS
        assert!(idx::inspector_ddl::YANK < INSPECTOR_DDL_KEYS.len());

        // CELL_EDIT_KEYS
        assert!(idx::cell_edit::WRITE < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::TYPE < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::MOVE < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::HOME_END < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::COMMAND < CELL_EDIT_KEYS.len());
        assert!(idx::cell_edit::ESC_CANCEL < CELL_EDIT_KEYS.len());

        // CONNECTION_SELECTOR_ROWS
        assert!(idx::connection_selector::CONFIRM < CONNECTION_SELECTOR_ROWS.len());
        assert!(idx::connection_selector::SELECT < CONNECTION_SELECTOR_ROWS.len());
        assert!(idx::connection_selector::NEW < CONNECTION_SELECTOR_ROWS.len());
        assert!(idx::connection_selector::EDIT < CONNECTION_SELECTOR_ROWS.len());
        assert!(idx::connection_selector::DELETE < CONNECTION_SELECTOR_ROWS.len());
        assert!(idx::connection_selector::CLOSE < CONNECTION_SELECTOR_ROWS.len());
    }

    #[test]
    fn help_content_line_count_matches_section_structure() {
        let sections: &[usize] = &[
            GLOBAL_KEYS.len(),
            NAVIGATION_KEYS.len(),
            HISTORY_KEYS.len(),
            RESULT_ACTIVE_KEYS.len(),
            INSPECTOR_DDL_KEYS.len(),
            CELL_EDIT_KEYS.len(),
            SQL_MODAL_KEYS.len(),
            OVERLAY_KEYS.len(),
            COMMAND_LINE_KEYS.len(),
            CONNECTION_SETUP_KEYS.len(),
            CONNECTION_ERROR_ROWS.len(),
            CONNECTION_SELECTOR_ROWS.len(),
            ER_PICKER_ROWS.len(),
            TABLE_PICKER_ROWS.len(),
            COMMAND_PALETTE_ROWS.len(),
            HELP_ROWS.len(),
            CONFIRM_DIALOG_KEYS.len(),
        ];
        let section_count = sections.len();
        let expected: usize = section_count + sections.iter().sum::<usize>() + (section_count - 1);

        assert_eq!(help_content_line_count(), expected);
    }

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
        #[case(idx::global::CONNECTIONS, Action::OpenConnectionSelector)]
        fn global_key_action_matches(#[case] i: usize, #[case] expected: Action) {
            assert!(
                std::mem::discriminant(&GLOBAL_KEYS[i].action) == std::mem::discriminant(&expected),
                "GLOBAL_KEYS[{i}] has action {:?}, expected {expected:?}",
                GLOBAL_KEYS[i].action
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

        // ------------------------------------------------------------------ //
        // 2. Non-None bindings have at least one combo (KeyBinding arrays)
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
            check_non_none_have_combos(HISTORY_KEYS, "HISTORY_KEYS");
        }

        // ------------------------------------------------------------------ //
        // 2b. ModeRow exec entries have non-empty combos
        // ------------------------------------------------------------------ //

        fn check_mode_rows_exec_valid(rows: &[ModeRow], name: &str) {
            for (i, row) in rows.iter().enumerate() {
                for (j, eb) in row.bindings.iter().enumerate() {
                    assert!(
                        !eb.combos.is_empty(),
                        "{name}[{i}].bindings[{j}] has action {:?} but no combos",
                        eb.action
                    );
                    assert!(
                        !matches!(eb.action, Action::None),
                        "{name}[{i}].bindings[{j}] has Action::None in exec binding",
                    );
                }
            }
        }

        #[test]
        fn all_mode_row_exec_entries_are_valid() {
            for (name, mb) in ALL_MODE_BINDINGS {
                check_mode_rows_exec_valid(mb.rows, name);
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

        fn check_no_duplicate_combos_rows(rows: &[ModeRow], name: &str) {
            let mut seen: Vec<KeyCombo> = Vec::new();
            for row in rows {
                for eb in row.bindings {
                    for combo in eb.combos {
                        if seen.contains(combo) {
                            panic!(
                                "{name}: duplicate combo {combo:?} in binding {:?}",
                                eb.action
                            );
                        }
                        seen.push(*combo);
                    }
                }
            }
        }

        // GLOBAL_KEYS excluded: FOCUS/EXIT_FOCUS share a combo for footer label switching.
        #[test]
        fn no_duplicate_combos_in_simple_modes() {
            check_no_duplicate_combos(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
            check_no_duplicate_combos(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
            for (name, mb) in ALL_MODE_BINDINGS {
                check_no_duplicate_combos_rows(mb.rows, name);
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

        fn check_resolve_mode_roundtrip(rows: &[ModeRow], name: &str) {
            for row in rows {
                for eb in row.bindings {
                    for combo in eb.combos {
                        let resolved = keymap::resolve_mode(combo, rows);
                        match resolved {
                            Some(ref action)
                                if std::mem::discriminant(action)
                                    == std::mem::discriminant(&eb.action) => {}
                            other => panic!(
                                "{name}: combo {combo:?} resolved to {other:?}, expected {:?}",
                                eb.action
                            ),
                        }
                    }
                }
            }
        }

        #[test]
        fn keymap_resolve_roundtrip_for_simple_modes() {
            check_keymap_roundtrip(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
            check_keymap_roundtrip(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
            for (name, mb) in ALL_MODE_BINDINGS {
                check_resolve_mode_roundtrip(mb.rows, name);
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

        fn check_no_plain_char_in_filter_mode_rows(
            rows: &[ModeRow],
            name: &str,
            allowed_chars: &[char],
        ) {
            let no_mods = Modifiers {
                ctrl: false,
                alt: false,
                shift: false,
            };
            for row in rows {
                for eb in row.bindings {
                    for combo in eb.combos {
                        if combo.modifiers == no_mods
                            && let Key::Char(c) = combo.key
                        {
                            assert!(
                                allowed_chars.contains(&c),
                                "{name}: executable entry {:?} has plain Char({c:?}) combo \
                                 which would shadow filter input",
                                eb.action
                            );
                        }
                    }
                }
            }
        }

        #[test]
        fn table_picker_has_no_plain_char_combos() {
            check_no_plain_char_in_filter_mode_rows(TABLE_PICKER_ROWS, "TABLE_PICKER_ROWS", &[]);
        }

        #[test]
        fn er_picker_has_no_plain_char_combos() {
            check_no_plain_char_in_filter_mode_rows(ER_PICKER_ROWS, "ER_PICKER_ROWS", &[' ']);
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
        // 6. Action::None entries must have combos: &[] (KeyBinding arrays only)
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
            check_none_action_entries_have_no_combos(HISTORY_KEYS, "HISTORY_KEYS");
        }

        // ------------------------------------------------------------------ //
        // 7. ALL_MODE_BINDINGS exhaustiveness
        // ------------------------------------------------------------------ //

        // HELP, CONNECTION_ERROR, TABLE_PICKER, ER_PICKER, COMMAND_PALETTE, CONNECTION_SELECTOR
        #[test]
        fn all_mode_bindings_count() {
            assert_eq!(ALL_MODE_BINDINGS.len(), 6);
        }
    }
}
