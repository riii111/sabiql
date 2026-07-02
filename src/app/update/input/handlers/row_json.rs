use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::input::keybindings::{Key, KeyCombo};

pub fn handle_row_json_keys(combo: KeyCombo) -> Action {
    match combo.key {
        Key::Esc => Action::CloseModal(ModalKind::RowJson),
        Key::Char('y') => Action::RowJsonYank,
        Key::Char('j') | Key::Down => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        },
        Key::Char('k') | Key::Up => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        },
        Key::Char('g') | Key::Home => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ToStart,
        },
        Key::Char('G') | Key::End => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::ToEnd,
        },
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::input::keybindings::KeyCombo;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    #[test]
    fn esc_closes() {
        let result = handle_row_json_keys(combo(Key::Esc));
        assert!(matches!(result, Action::CloseModal(ModalKind::RowJson)));
    }

    #[test]
    fn y_yanks() {
        let result = handle_row_json_keys(combo(Key::Char('y')));
        assert!(matches!(result, Action::RowJsonYank));
    }

    #[test]
    fn j_scrolls_down() {
        let result = handle_row_json_keys(combo(Key::Char('j')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowJson,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn k_scrolls_up() {
        let result = handle_row_json_keys(combo(Key::Char('k')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowJson,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn g_scrolls_to_start() {
        let result = handle_row_json_keys(combo(Key::Char('g')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowJson,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::ToStart,
            }
        ));
    }

    #[test]
    fn shift_g_scrolls_to_end() {
        let result = handle_row_json_keys(combo(Key::Char('G')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowJson,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::ToEnd,
            }
        ));
    }

    #[test]
    fn unknown_key_is_noop() {
        let result = handle_row_json_keys(combo(Key::Char('x')));
        assert!(matches!(result, Action::None));
    }
}
