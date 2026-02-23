use super::action::Action;
use super::keybindings::{KeyBinding, KeyCombo};

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

    fn none_j_binding() -> KeyBinding {
        KeyBinding {
            key_short: "j",
            key: "j",
            desc_short: "Nav",
            description: "Navigate",
            action: Action::None,
            combos: J_COMBOS,
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
        let none_j = none_j_binding();
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
}
