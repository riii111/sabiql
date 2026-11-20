use crate::model::app_state::AppState;
use crate::ports::inbound::KeyCombo;
use crate::update::action::Action;
use crate::update::input::keymap::{KeyContext, cell_detail_action};

pub(super) fn handle_cell_detail_keys(combo: KeyCombo, state: &AppState) -> Action {
    cell_detail_action(combo, &KeyContext::from_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::input_mode::InputMode;
    use crate::ports::inbound::{Key, Modifiers};
    use crate::update::action::DetailTarget;
    use crate::update::action::SearchStep;
    use crate::update::action::{CursorMove, InputTarget};
    use crate::update::action::{ScrollAmount, ScrollDirection, ScrollTarget};

    fn handle_cell_detail_keys(combo: KeyCombo, searching: bool) -> Action {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::CellDetail);
        if searching {
            state.cell_detail.enter_search();
        }
        super::handle_cell_detail_keys(combo, &state)
    }

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    fn modified_combo(k: Key, modifiers: Modifiers) -> KeyCombo {
        KeyCombo { key: k, modifiers }
    }

    #[test]
    fn enter_confirms_active_search() {
        let result = handle_cell_detail_keys(combo(Key::Enter), true);

        assert!(matches!(
            result,
            Action::DetailSearch {
                target: DetailTarget::CellDetail,
                step: SearchStep::Submit,
            }
        ));
    }

    #[test]
    fn ctrl_char_is_not_inserted_into_search() {
        let result = handle_cell_detail_keys(modified_combo(Key::Char('n'), Modifiers::CTRL), true);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn alt_char_is_inserted_into_search() {
        let result = handle_cell_detail_keys(modified_combo(Key::Char('n'), Modifiers::ALT), true);

        assert!(matches!(
            result,
            Action::TextInput {
                target: InputTarget::CellDetailSearch,
                ch: 'n'
            }
        ));
    }

    #[test]
    fn readline_moves_to_start_while_searching() {
        let result = handle_cell_detail_keys(KeyCombo::ctrl(Key::Char('a')), true);
        assert!(matches!(
            result,
            Action::TextMoveCursor {
                target: InputTarget::CellDetailSearch,
                direction: CursorMove::LineStart
            }
        ));
    }

    #[test]
    fn shifted_arrow_keeps_moving_the_search_cursor() {
        let result = handle_cell_detail_keys(KeyCombo::shift(Key::Left), true);
        assert!(matches!(
            result,
            Action::TextMoveCursor {
                target: InputTarget::CellDetailSearch,
                direction: CursorMove::Left
            }
        ));
    }

    #[test]
    fn j_scrolls_down_when_not_searching() {
        let result = handle_cell_detail_keys(combo(Key::Char('j')), false);

        assert!(matches!(
            result,
            Action::Scroll {
                target: ScrollTarget::CellDetail,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            }
        ));
    }

    #[test]
    fn slash_enters_search_when_not_searching() {
        let result = handle_cell_detail_keys(combo(Key::Char('/')), false);

        assert!(matches!(
            result,
            Action::DetailSearch {
                target: DetailTarget::CellDetail,
                step: SearchStep::Enter,
            }
        ));
    }
}
