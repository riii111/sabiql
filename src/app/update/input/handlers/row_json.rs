use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::input::keybindings::{Key, KeyCombo, Modifiers};

pub fn handle_row_json_keys(combo: KeyCombo) -> Action {
    match (combo.key, combo.modifiers) {
        (Key::Esc, _) => Action::CloseModal(ModalKind::RowJson),
        (Key::Char('y'), _) => Action::RowJsonYank,
        (Key::Char('j') | Key::Down, _) => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        },
        (Key::Char('k') | Key::Up, _) => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        },
        (Key::PageDown, _) | (Key::Char('f'), Modifiers::CTRL) => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::FullPage,
        },
        (Key::PageUp, _) | (Key::Char('b'), Modifiers::CTRL) => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::FullPage,
        },
        (Key::Char('g') | Key::Home, _) => Action::Scroll {
            target: ScrollTarget::RowJson,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ToStart,
        },
        (Key::Char('G') | Key::End, _) => Action::Scroll {
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
    use crate::update::action::Action;
    use crate::update::input::keybindings::{Key, KeyCombo};

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
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

    #[test]
    fn page_down_scrolls_full_page_down() {
        let result = handle_row_json_keys(combo(Key::PageDown));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowJson,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::FullPage,
            }
        ));
    }

    #[test]
    fn page_up_scrolls_full_page_up() {
        let result = handle_row_json_keys(combo(Key::PageUp));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowJson,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::FullPage,
            }
        ));
    }

    #[test]
    fn ctrl_f_scrolls_full_page_down() {
        let result = handle_row_json_keys(combo_ctrl(Key::Char('f')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowJson,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::FullPage,
            }
        ));
    }

    #[test]
    fn ctrl_b_scrolls_full_page_up() {
        let result = handle_row_json_keys(combo_ctrl(Key::Char('b')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowJson,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::FullPage,
            }
        ));
    }
}
