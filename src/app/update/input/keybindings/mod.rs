mod connections;
mod editors;
mod normal;
mod overlays;

use crate::model::shared::settings::KeymapPreset;
pub use crate::ports::inbound::{Key, KeyCombo, Modifiers};
use crate::update::action::{Action, ModalKind};
use crate::update::input::keymap::resolve_mode;
pub use connections::*;
pub use editors::*;
pub use normal::*;
pub use overlays::*;

#[derive(Clone)]
pub struct KeyBinding {
    pub key_short: &'static str,
    pub key: &'static str,
    pub desc_short: &'static str,
    pub description: &'static str,
    pub action: Action,
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

pub struct ModeBindings {
    pub rows: &'static [ModeRow],
}

impl ModeBindings {
    pub fn resolve(&self, combo: &KeyCombo) -> Option<Action> {
        resolve_mode(combo, self.rows)
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
pub const QUERY_HISTORY_PICKER: ModeBindings = ModeBindings {
    rows: QUERY_HISTORY_PICKER_ROWS,
};
pub const COMMAND_PALETTE: ModeBindings = ModeBindings {
    rows: COMMAND_PALETTE_ROWS,
};
pub const SETTINGS: ModeBindings = ModeBindings {
    rows: SETTINGS_ROWS,
};
pub const CONNECTION_SELECTOR: ModeBindings = ModeBindings {
    rows: CONNECTION_SELECTOR_ROWS,
};
pub const JSONB_DETAIL: ModeBindings = ModeBindings {
    rows: JSONB_DETAIL_ROWS,
};
pub const JSONB_EDIT: ModeBindings = ModeBindings {
    rows: JSONB_EDIT_ROWS,
};
pub const CELL_DETAIL: ModeBindings = ModeBindings {
    rows: CELL_DETAIL_ROWS,
};

pub const ALL_MODE_BINDINGS: &[(&str, &ModeBindings)] = &[
    ("HELP", &HELP),
    ("CONNECTION_ERROR", &CONNECTION_ERROR),
    ("TABLE_PICKER", &TABLE_PICKER),
    ("ER_PICKER", &ER_PICKER),
    ("QUERY_HISTORY_PICKER", &QUERY_HISTORY_PICKER),
    ("COMMAND_PALETTE", &COMMAND_PALETTE),
    ("SETTINGS", &SETTINGS),
    ("CONNECTION_SELECTOR", &CONNECTION_SELECTOR),
    ("JSONB_DETAIL", &JSONB_DETAIL),
    ("JSONB_EDIT", &JSONB_EDIT),
    ("CELL_DETAIL", &CELL_DETAIL),
];

pub const HELP_KEY_INDENT_WIDTH: usize = 2;
pub const HELP_KEY_DESC_GAP: usize = 2;

pub fn global_action_for(combo: &KeyCombo, preset: KeymapPreset) -> Option<Action> {
    normal::global_keys_for(preset)
        .iter()
        .filter(|binding| {
            !matches!(
                &binding.action,
                Action::OpenModal(
                    ModalKind::SqlModal | ModalKind::ErTablePicker | ModalKind::ConnectionSelector
                )
            )
        })
        .find(|binding| binding.combos.contains(combo))
        .map(|binding| binding.action.clone())
}

// Action has payload variants without PartialEq, so tests compare by
// discriminant — except modal actions, where several bindings differ only by
// ModalKind and the kind must participate in equality.
#[cfg(test)]
pub fn same_payload_free_action(actual: &Action, expected: &Action) -> bool {
    match (actual, expected) {
        (Action::OpenModal(a), Action::OpenModal(b))
        | (Action::CloseModal(a), Action::CloseModal(b))
        | (Action::ToggleModal(a), Action::ToggleModal(b)) => a == b,
        _ => std::mem::discriminant(actual) == std::mem::discriminant(expected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod catalog_semantics {
        use super::*;
        use crate::update::input::keymap;

        mod action_mapping {
            use super::*;
            use crate::update::action::{ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
            use rstest::rstest;

            #[rstest]
            #[case(global::QUIT, Action::Quit)]
            #[case(global::HELP, Action::ToggleModal(ModalKind::Help))]
            #[case(global::TABLE_PICKER, Action::OpenModal(ModalKind::TablePicker))]
            #[case(global::SETTINGS, Action::OpenModal(ModalKind::Settings))]
            #[case(global::COMMAND_LINE, Action::EnterCommandLine)]
            #[case(global::COMMAND_PALETTE, Action::OpenModal(ModalKind::CommandPalette))]
            #[case(global::RELOAD, Action::ReloadMetadata)]
            #[case(global::SQL, Action::OpenModal(ModalKind::SqlModal))]
            #[case(global::ER_DIAGRAM, Action::OpenModal(ModalKind::ErTablePicker))]
            #[case(global::CONNECTIONS, Action::OpenModal(ModalKind::ConnectionSelector))]
            #[case(global::CSV_EXPORT, Action::RequestCsvExport)]
            #[case(global::READ_ONLY, Action::ToggleReadOnly)]
            #[case(global::EXIT_READ_ONLY, Action::ToggleReadOnly)]
            #[case(
                global::QUERY_HISTORY,
                Action::OpenModal(ModalKind::QueryHistoryPicker)
            )]
            fn global_key_action_matches(#[case] kb: KeyBinding, #[case] expected: Action) {
                assert_payload_free_action_eq(&kb, &expected);
            }

            #[rstest]
            #[case(sql_modal_plan::EXPLAIN, Action::ExplainRequest)]
            #[case(sql_modal_plan::ANALYZE, Action::ExplainAnalyzeRequest)]
            #[case(sql_modal_plan::YANK, Action::SqlModalYank)]
            #[case(sql_modal_plan::TAB, Action::SqlModalNextTab)]
            #[case(sql_modal_plan::BACKTAB, Action::SqlModalPrevTab)]
            #[case(sql_modal_plan::CLOSE, Action::CloseModal(ModalKind::SqlModal))]
            fn plan_key_action_matches(#[case] kb: KeyBinding, #[case] expected: Action) {
                assert_payload_free_action_eq(&kb, &expected);
            }

            #[rstest]
            #[case(sql_modal_compare::EXPLAIN, Action::ExplainRequest)]
            #[case(sql_modal_compare::ANALYZE, Action::ExplainAnalyzeRequest)]
            #[case(sql_modal_compare::EDIT_QUERY, Action::CompareEditQuery)]
            #[case(sql_modal_compare::YANK, Action::SqlModalYank)]
            #[case(sql_modal_compare::TAB, Action::SqlModalNextTab)]
            #[case(sql_modal_compare::BACKTAB, Action::SqlModalPrevTab)]
            #[case(sql_modal_compare::CLOSE, Action::CloseModal(ModalKind::SqlModal))]
            fn compare_key_action_matches(#[case] kb: KeyBinding, #[case] expected: Action) {
                assert_payload_free_action_eq(&kb, &expected);
            }

            #[test]
            fn confirm_yes_action_matches() {
                assert!(matches!(confirm::YES.action, Action::ConfirmDialogConfirm));
            }

            #[test]
            fn confirm_scroll_down_action_matches() {
                assert!(matches!(
                    confirm::SCROLL_DOWN.action,
                    Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    }
                ));
            }

            #[test]
            fn confirm_scroll_up_action_matches() {
                assert!(matches!(
                    confirm::SCROLL_UP.action,
                    Action::Scroll {
                        target: ScrollTarget::ConfirmDialog,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    }
                ));
            }

            #[test]
            fn confirm_no_action_matches() {
                assert!(matches!(confirm::NO.action, Action::ConfirmDialogCancel));
            }

            fn assert_payload_free_action_eq(kb: &KeyBinding, expected: &Action) {
                assert!(
                    same_payload_free_action(&kb.action, expected),
                    "binding '{}' ({}) has action {:?}, expected {expected:?}",
                    kb.key,
                    kb.description,
                    kb.action,
                );
            }
        }

        mod binding_shape {
            use super::*;

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
                check_non_none_have_combos(IDE_GLOBAL_KEYS, "IDE_GLOBAL_KEYS");
                check_non_none_have_combos(CONFIRM_DIALOG_KEYS, "CONFIRM_DIALOG_KEYS");
                check_non_none_have_combos(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS");
                check_non_none_have_combos(CELL_EDIT_KEYS, "CELL_EDIT_KEYS");
                check_non_none_have_combos(JSONB_SEARCH_KEYS, "JSONB_SEARCH_KEYS");
                check_non_none_have_combos(CELL_DETAIL_SEARCH_KEYS, "CELL_DETAIL_SEARCH_KEYS");
            }

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
                check_mode_rows_exec_valid(ER_PICKER_ROWS_IDE, "ER_PICKER_ROWS_IDE");
            }

            fn check_none_action_entries_have_no_combos(bindings: &[KeyBinding], name: &str) {
                for (i, kb) in bindings.iter().enumerate() {
                    assert!(
                        !matches!(kb.action, Action::None) || kb.combos.is_empty(),
                        "{name}[{i}] has action Action::None but non-empty combos: {:?}",
                        kb.combos
                    );
                }
            }

            #[test]
            fn none_action_entries_have_no_combos() {
                check_none_action_entries_have_no_combos(GLOBAL_KEYS, "GLOBAL_KEYS");
                check_none_action_entries_have_no_combos(NAVIGATION_KEYS, "NAVIGATION_KEYS");
                check_none_action_entries_have_no_combos(FOOTER_NAV_KEYS, "FOOTER_NAV_KEYS");
                check_none_action_entries_have_no_combos(
                    SQL_MODAL_NORMAL_KEYS,
                    "SQL_MODAL_NORMAL_KEYS",
                );
                check_none_action_entries_have_no_combos(SQL_MODAL_KEYS, "SQL_MODAL_KEYS");
                check_none_action_entries_have_no_combos(
                    SQL_MODAL_PLAN_KEYS,
                    "SQL_MODAL_PLAN_KEYS",
                );
                check_none_action_entries_have_no_combos(
                    SQL_MODAL_COMPARE_KEYS,
                    "SQL_MODAL_COMPARE_KEYS",
                );
                check_none_action_entries_have_no_combos(
                    SQL_MODAL_CONFIRMING_KEYS,
                    "SQL_MODAL_CONFIRMING_KEYS",
                );
                check_none_action_entries_have_no_combos(OVERLAY_KEYS, "OVERLAY_KEYS");
                check_none_action_entries_have_no_combos(
                    CONNECTION_SETUP_KEYS,
                    "CONNECTION_SETUP_KEYS",
                );
                check_none_action_entries_have_no_combos(RESULT_ACTIVE_KEYS, "RESULT_ACTIVE_KEYS");
                check_none_action_entries_have_no_combos(JSONB_SEARCH_KEYS, "JSONB_SEARCH_KEYS");
                check_none_action_entries_have_no_combos(
                    CELL_DETAIL_SEARCH_KEYS,
                    "CELL_DETAIL_SEARCH_KEYS",
                );
            }
        }

        mod uniqueness {
            use super::*;

            fn check_no_duplicate_combos(bindings: &[KeyBinding], name: &str) {
                let mut seen: Vec<KeyCombo> = Vec::new();
                for kb in bindings
                    .iter()
                    .filter(|kb| !matches!(kb.action, Action::None))
                {
                    for combo in kb.combos {
                        assert!(
                            !seen.contains(combo),
                            "{name}: duplicate combo {combo:?} in binding {:?}",
                            kb.action
                        );
                        seen.push(*combo);
                    }
                }
            }

            fn check_no_conflicting_combos(bindings: &[KeyBinding], name: &str) {
                let mut seen: Vec<(&KeyCombo, &Action)> = Vec::new();
                for kb in bindings
                    .iter()
                    .filter(|kb| !matches!(kb.action, Action::None))
                {
                    for combo in kb.combos {
                        if let Some((_, action)) = seen.iter().find(|(seen, _)| *seen == combo) {
                            assert!(
                                same_payload_free_action(action, &kb.action),
                                "{name}: combo {combo:?} maps to both {action:?} and {:?}",
                                kb.action
                            );
                        }
                        seen.push((combo, &kb.action));
                    }
                }
            }

            fn check_no_duplicate_combos_rows(rows: &[ModeRow], name: &str) {
                let mut seen: Vec<KeyCombo> = Vec::new();
                for row in rows {
                    for eb in row.bindings {
                        for combo in eb.combos {
                            assert!(
                                !seen.contains(combo),
                                "{name}: duplicate combo {combo:?} in binding {:?}",
                                eb.action
                            );
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
                check_no_duplicate_combos(JSONB_SEARCH_KEYS, "JSONB_SEARCH_KEYS");
                check_no_duplicate_combos(CELL_DETAIL_SEARCH_KEYS, "CELL_DETAIL_SEARCH_KEYS");
                check_no_conflicting_combos(GLOBAL_KEYS, "GLOBAL_KEYS");
                check_no_conflicting_combos(IDE_GLOBAL_KEYS, "IDE_GLOBAL_KEYS");
                for (name, mb) in ALL_MODE_BINDINGS {
                    check_no_duplicate_combos_rows(mb.rows, name);
                }
                check_no_duplicate_combos_rows(ER_PICKER_ROWS_IDE, "ER_PICKER_ROWS_IDE");
            }
        }

        mod resolver_contract {
            use super::*;

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
                check_keymap_roundtrip(JSONB_SEARCH_KEYS, "JSONB_SEARCH_KEYS");
                check_keymap_roundtrip(CELL_DETAIL_SEARCH_KEYS, "CELL_DETAIL_SEARCH_KEYS");
                check_keymap_roundtrip(GLOBAL_KEYS, "GLOBAL_KEYS");
                check_keymap_roundtrip(IDE_GLOBAL_KEYS, "IDE_GLOBAL_KEYS");
                for (name, mb) in ALL_MODE_BINDINGS {
                    check_resolve_mode_roundtrip(mb.rows, name);
                }
                check_resolve_mode_roundtrip(ER_PICKER_ROWS_IDE, "ER_PICKER_ROWS_IDE");
            }
        }

        mod conflict_safety {
            use super::*;

            fn check_no_plain_char_in_filter_mode(
                bindings: &[KeyBinding],
                name: &str,
                allowed_chars: &[char],
            ) {
                let no_mods = Modifiers::empty();
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
                let no_mods = Modifiers::empty();
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
                check_no_plain_char_in_filter_mode_rows(
                    TABLE_PICKER_ROWS,
                    "TABLE_PICKER_ROWS",
                    &[],
                );
            }

            #[test]
            fn er_picker_has_no_plain_char_combos() {
                check_no_plain_char_in_filter_mode_rows(ER_PICKER_ROWS, "ER_PICKER_ROWS", &[' ']);
                check_no_plain_char_in_filter_mode_rows(
                    ER_PICKER_ROWS_IDE,
                    "ER_PICKER_ROWS_IDE",
                    &[' '],
                );
            }

            #[test]
            fn query_history_picker_has_no_plain_char_combos() {
                check_no_plain_char_in_filter_mode_rows(
                    QUERY_HISTORY_PICKER_ROWS,
                    "QUERY_HISTORY_PICKER_ROWS",
                    &[],
                );
            }

            #[test]
            fn command_line_has_no_problematic_plain_char_combos() {
                check_no_plain_char_in_filter_mode(COMMAND_LINE_KEYS, "COMMAND_LINE_KEYS", &[]);
            }

            #[test]
            fn cell_edit_plain_char_combos_are_intentional() {
                check_no_plain_char_in_filter_mode(CELL_EDIT_KEYS, "CELL_EDIT_KEYS", &[':']);
            }
        }

        mod catalog_coverage {
            use super::*;

            #[test]
            fn all_mode_bindings_count() {
                assert_eq!(ALL_MODE_BINDINGS.len(), 11);
            }
        }
    }
}
