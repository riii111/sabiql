use super::action::Action;
use super::keybindings::{KeyBinding, KeyCombo, ModeRow};

/// Look up the action for a `KeyCombo` in a binding array.
///
/// Returns `Some(action)` if a non-`Action::None` entry has a matching combo,
/// otherwise `None`. Display-only entries (`Action::None`) are skipped.
pub fn resolve(combo: &KeyCombo, bindings: &[KeyBinding]) -> Option<Action> {
    bindings
        .iter()
        .filter(|kb| !matches!(kb.action, Action::None))
        .find(|kb| kb.combos.contains(combo))
        .map(|kb| kb.action.clone())
}

/// Look up the action for a `KeyCombo` in a `ModeRow` slice.
pub fn resolve_mode(combo: &KeyCombo, rows: &[ModeRow]) -> Option<Action> {
    for row in rows {
        for eb in row.bindings {
            if !matches!(eb.action, Action::None) && eb.combos.contains(combo) {
                return Some(eb.action.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keybindings::{Key, KeyCombo};

    static QUIT_COMBOS: &[KeyCombo] = &[KeyCombo::plain(Key::Char('q'))];
    static HELP_COMBOS: &[KeyCombo] = &[KeyCombo::plain(Key::Char('?'))];
    static J_COMBOS: &[KeyCombo] = &[KeyCombo::plain(Key::Char('j'))];
    static EMPTY_COMBOS: &[KeyCombo] = &[];

    fn quit_binding() -> KeyBinding {
        KeyBinding {
            key_short: "q",
            key: "q",
            desc_short: "Quit",
            description: "Quit",
            action: Action::Quit,
            combos: QUIT_COMBOS,
        }
    }

    fn none_display_binding() -> KeyBinding {
        KeyBinding {
            key_short: "j/k",
            key: "j/k",
            desc_short: "Nav",
            description: "Navigate",
            action: Action::None,
            combos: J_COMBOS, // mimics executable-array display entry (combos as metadata)
        }
    }

    fn help_binding() -> KeyBinding {
        KeyBinding {
            key_short: "?",
            key: "?",
            desc_short: "Help",
            description: "Help",
            action: Action::OpenHelp,
            combos: HELP_COMBOS,
        }
    }

    fn empty_combos_binding() -> KeyBinding {
        KeyBinding {
            key_short: "q",
            key: "q",
            desc_short: "Quit",
            description: "Quit",
            action: Action::Quit,
            combos: EMPTY_COMBOS,
        }
    }

    #[test]
    fn resolves_matching_combo() {
        let bindings = [quit_binding()];

        let result = resolve(&KeyCombo::plain(Key::Char('q')), &bindings);

        assert!(matches!(result, Some(Action::Quit)));
    }

    #[test]
    fn returns_none_for_no_match() {
        let bindings = [quit_binding()];

        let result = resolve(&KeyCombo::plain(Key::Char('x')), &bindings);

        assert!(result.is_none());
    }

    #[test]
    fn skips_display_only_none_entries() {
        let none_j = none_display_binding();
        let quit = quit_binding();
        let bindings = [none_j, quit];

        // 'j' is in a None entry — should not match
        assert!(resolve(&KeyCombo::plain(Key::Char('j')), &bindings).is_none());
        // 'q' matches the real entry
        assert!(matches!(
            resolve(&KeyCombo::plain(Key::Char('q')), &bindings),
            Some(Action::Quit)
        ));
    }

    #[test]
    fn returns_first_matching_non_none_entry() {
        let quit = quit_binding();
        let help = help_binding();
        let bindings = [quit, help];

        // Quit combo matches first
        let result = resolve(&KeyCombo::plain(Key::Char('q')), &bindings);

        assert!(matches!(result, Some(Action::Quit)));
    }

    #[test]
    fn empty_combos_entry_never_matches() {
        let bindings = [empty_combos_binding()];

        let result = resolve(&KeyCombo::plain(Key::Char('q')), &bindings);

        assert!(result.is_none());
    }

    mod resolve_mode_tests {
        use super::*;
        use crate::app::keybindings::{CONNECTION_ERROR_ROWS, HELP_ROWS, TABLE_PICKER_ROWS};

        #[test]
        fn empty_rows_returns_none() {
            let result = resolve_mode(&KeyCombo::plain(Key::Char('q')), &[]);

            assert!(result.is_none());
        }

        #[test]
        fn matches_binding_in_rows() {
            let result = resolve_mode(&KeyCombo::plain(Key::Esc), HELP_ROWS);

            assert!(matches!(result, Some(Action::CloseHelp)));
        }

        #[test]
        fn no_match_returns_none() {
            let result = resolve_mode(&KeyCombo::plain(Key::F(12)), HELP_ROWS);

            assert!(result.is_none());
        }

        // CONNECTION_ERROR_ROWS has multiple bindings; Esc at idx 5 resolves to CloseConnectionError
        #[test]
        fn first_matching_binding_wins() {
            let result = resolve_mode(&KeyCombo::plain(Key::Esc), CONNECTION_ERROR_ROWS);

            assert!(matches!(result, Some(Action::CloseConnectionError)));
        }

        // TYPE_FILTER row has no Enter combo — Enter resolves to ConfirmSelection at idx 0
        #[test]
        fn unrelated_row_does_not_block_later_match() {
            let result = resolve_mode(&KeyCombo::plain(Key::Enter), TABLE_PICKER_ROWS);

            assert!(matches!(result, Some(Action::ConfirmSelection)));
        }
    }
}
