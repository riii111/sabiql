mod editors;
mod normal;
mod overlays;
mod readline;

use crate::model::shared::settings::KeymapPreset;
use crate::policy::{FeaturePolicy, FeatureRequirement};
pub use crate::ports::inbound::{Key, KeyCombo, Modifiers};
use crate::update::action::{Action, ModalKind};
pub use editors::*;
pub use normal::*;
pub use overlays::*;
pub use readline::*;

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

    pub fn feature_requirement(&self) -> FeatureRequirement {
        self.action.feature_requirement()
    }
}

pub fn global_action_for_with_policy(
    combo: &KeyCombo,
    preset: KeymapPreset,
    feature_policy: &FeaturePolicy,
) -> Option<Action> {
    normal::global_keys_for(preset)
        .iter()
        .filter(|binding| {
            feature_policy.is_enabled(binding.feature_requirement())
                && !matches!(
                    &binding.action,
                    Action::OpenModal(
                        ModalKind::SqlModal
                            | ModalKind::ErTablePicker
                            | ModalKind::ConnectionSelector
                    )
                )
        })
        .find(|binding| binding.combos.contains(combo))
        .map(|binding| binding.action.clone())
}

#[cfg(test)]
pub mod test_support {
    use super::Action;

    // Action has payload variants without PartialEq, so tests compare by discriminant.
    // Bindings that differ only by a modal kind, detail target, or search step compare that payload too.
    pub fn same_payload_free_action(actual: &Action, expected: &Action) -> bool {
        match (actual, expected) {
            (Action::OpenModal(a), Action::OpenModal(b))
            | (Action::CloseModal(a), Action::CloseModal(b)) => a == b,
            (
                Action::DetailSearch {
                    target: a_target,
                    step: a_step,
                },
                Action::DetailSearch {
                    target: b_target,
                    step: b_step,
                },
            ) => a_target == b_target && a_step == b_step,
            (Action::DetailYankAll(a), Action::DetailYankAll(b)) => a == b,
            _ => std::mem::discriminant(actual) == std::mem::discriminant(expected),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::same_payload_free_action;
    use super::*;

    mod catalog_semantics {
        use super::*;

        mod action_mapping {
            use super::*;
            use crate::update::action::ModalKind;
            use rstest::rstest;

            #[rstest]
            #[case(global::QUIT, Action::Quit)]
            #[case(global::HELP, Action::ToggleHelp)]
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
            #[case(
                global::SQLITE_DIAGNOSTICS,
                Action::OpenModal(ModalKind::SqliteDiagnostics)
            )]
            fn global_key_action_matches(#[case] kb: KeyBinding, #[case] expected: Action) {
                assert_payload_free_action_eq(&kb, &expected);
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
                check_none_action_entries_have_no_combos(OVERLAY_KEYS, "OVERLAY_KEYS");
                check_none_action_entries_have_no_combos(RESULT_ACTIVE_KEYS, "RESULT_ACTIVE_KEYS");
            }
        }

        mod uniqueness {
            use super::*;

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

            // GLOBAL_KEYS excluded: FOCUS/EXIT_FOCUS share a combo for footer label switching.
            #[test]
            fn no_duplicate_combos_in_simple_modes() {
                check_no_conflicting_combos(GLOBAL_KEYS, "GLOBAL_KEYS");
                check_no_conflicting_combos(IDE_GLOBAL_KEYS, "IDE_GLOBAL_KEYS");
            }
        }
    }

    #[test]
    fn feature_requirements_are_attached_to_feature_bindings() {
        assert_eq!(
            global::ER_DIAGRAM.feature_requirement(),
            FeatureRequirement::ErDiagram
        );
        assert_eq!(
            global::SQLITE_DIAGNOSTICS.feature_requirement(),
            FeatureRequirement::SqliteDiagnostics
        );
    }
}
