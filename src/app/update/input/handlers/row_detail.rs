use crate::update::action::Action;
use crate::update::input::keybindings::{KeyCombo, ROW_DETAIL};

pub fn handle_row_detail_keys(combo: KeyCombo) -> Action {
    ROW_DETAIL.resolve(&combo).unwrap_or(Action::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::action::{Action, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget};
    use crate::update::input::keybindings::{Key, KeyCombo};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn combo_ctrl(k: Key) -> KeyCombo {
        KeyCombo::ctrl(k)
    }

    fn combo_alt(k: Key) -> KeyCombo {
        KeyCombo::alt(k)
    }

    #[test]
    fn esc_closes() {
        let result = handle_row_detail_keys(combo(Key::Esc));
        assert!(matches!(result, Action::CloseModal(ModalKind::RowDetail)));
    }

    #[test]
    fn y_yanks_display_text() {
        let result = handle_row_detail_keys(combo(Key::Char('y')));
        assert!(matches!(result, Action::RowDetailYank));
    }

    #[test]
    fn shift_y_yanks_json() {
        let result = handle_row_detail_keys(combo(Key::Char('Y')));
        assert!(matches!(result, Action::RowDetailYankJson));
    }

    #[rstest]
    #[case(combo_ctrl(Key::Char('y')))]
    #[case(combo_ctrl(Key::Char('Y')))]
    #[case(combo_alt(Key::Char('y')))]
    #[case(combo_alt(Key::Char('Y')))]
    fn modified_y_keys_do_not_copy(#[case] input: KeyCombo) {
        let result = handle_row_detail_keys(input);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn j_scrolls_down() {
        let result = handle_row_detail_keys(combo(Key::Char('j')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn k_scrolls_up() {
        let result = handle_row_detail_keys(combo(Key::Char('k')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[rstest]
    #[case(combo(Key::Char('h')))]
    #[case(combo(Key::Left))]
    fn horizontal_left_scrolls_left(#[case] input: KeyCombo) {
        let result = handle_row_detail_keys(input);
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Left,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[rstest]
    #[case(combo(Key::Char('l')))]
    #[case(combo(Key::Right))]
    fn horizontal_right_scrolls_right(#[case] input: KeyCombo) {
        let result = handle_row_detail_keys(input);
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Right,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn g_scrolls_to_start() {
        let result = handle_row_detail_keys(combo(Key::Char('g')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::ToStart,
            }
        ));
    }

    #[test]
    fn shift_g_scrolls_to_end() {
        let result = handle_row_detail_keys(combo(Key::Char('G')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::ToEnd,
            }
        ));
    }

    #[test]
    fn unknown_key_is_noop() {
        let result = handle_row_detail_keys(combo(Key::Char('x')));
        assert!(matches!(result, Action::None));
    }

    #[test]
    fn page_down_scrolls_full_page_down() {
        let result = handle_row_detail_keys(combo(Key::PageDown));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::FullPage,
            }
        ));
    }

    #[test]
    fn page_up_scrolls_full_page_up() {
        let result = handle_row_detail_keys(combo(Key::PageUp));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::FullPage,
            }
        ));
    }

    #[test]
    fn ctrl_f_scrolls_full_page_down() {
        let result = handle_row_detail_keys(combo_ctrl(Key::Char('f')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::FullPage,
            }
        ));
    }

    #[test]
    fn ctrl_d_scrolls_half_page_down() {
        let result = handle_row_detail_keys(combo_ctrl(Key::Char('d')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::HalfPage,
            }
        ));
    }

    #[test]
    fn ctrl_u_scrolls_half_page_up() {
        let result = handle_row_detail_keys(combo_ctrl(Key::Char('u')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::HalfPage,
            }
        ));
    }

    #[test]
    fn ctrl_b_scrolls_full_page_up() {
        let result = handle_row_detail_keys(combo_ctrl(Key::Char('b')));
        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::RowDetail,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::FullPage,
            }
        ));
    }
}
