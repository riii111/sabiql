use crate::app::action::Action;
use crate::app::keybindings::{Key, KeyCombo};
use crate::app::sql_modal_context::SqlModalStatus;

pub fn handle_sql_modal_keys(
    combo: KeyCombo,
    completion_visible: bool,
    status: &SqlModalStatus,
) -> Action {
    use crate::app::action::CursorMove;

    if matches!(status, SqlModalStatus::ConfirmingHigh { .. }) {
        let plain = !combo.modifiers.ctrl && !combo.modifiers.alt;
        return match combo.key {
            Key::Char(c) if plain => Action::SqlModalHighRiskInput(c),
            Key::Backspace if plain => Action::SqlModalHighRiskBackspace,
            Key::Left => Action::SqlModalHighRiskMoveCursor(CursorMove::Left),
            Key::Right => Action::SqlModalHighRiskMoveCursor(CursorMove::Right),
            Key::Home => Action::SqlModalHighRiskMoveCursor(CursorMove::Home),
            Key::End => Action::SqlModalHighRiskMoveCursor(CursorMove::End),
            Key::Enter if plain => Action::SqlModalHighRiskConfirmExecute,
            Key::Esc => Action::SqlModalCancelConfirm,
            _ => Action::None,
        };
    }

    // In Confirming state only plain Enter/Esc are meaningful; all other keys are ignored
    // to prevent accidental edits while the risk warning is displayed.
    // Alt+Enter (submit shortcut) is intentionally excluded — only explicit plain Enter confirms.
    if matches!(status, SqlModalStatus::Confirming(_)) {
        let plain = !combo.modifiers.ctrl && !combo.modifiers.alt;
        return match combo.key {
            Key::Enter if plain => Action::SqlModalConfirmExecute,
            Key::Esc => Action::SqlModalCancelConfirm,
            _ => Action::None,
        };
    }

    let ctrl = combo.modifiers.ctrl;
    let alt = combo.modifiers.alt;

    if alt && combo.key == Key::Enter {
        return Action::SqlModalSubmit;
    }

    if ctrl && combo.key == Key::Char(' ') {
        return Action::CompletionTrigger;
    }

    if ctrl && combo.key == Key::Char('l') {
        return Action::SqlModalClear;
    }

    match (combo.key, completion_visible) {
        // Completion navigation (when popup is visible)
        (Key::Up, true) => Action::CompletionPrev,
        (Key::Down, true) => Action::CompletionNext,
        (Key::Tab | Key::Enter, true) => Action::CompletionAccept,
        (Key::Esc, true) => Action::CompletionDismiss,
        // Navigation: dismiss completion on horizontal movement
        (Key::Left | Key::Right, true) => Action::CompletionDismiss,

        // Esc: Close modal (when completion not visible)
        (Key::Esc, false) => Action::CloseSqlModal,
        (Key::Left, false) => Action::SqlModalMoveCursor(CursorMove::Left),
        (Key::Right, false) => Action::SqlModalMoveCursor(CursorMove::Right),
        (Key::Up, false) => Action::SqlModalMoveCursor(CursorMove::Up),
        (Key::Down, false) => Action::SqlModalMoveCursor(CursorMove::Down),
        (Key::Home, _) => Action::SqlModalMoveCursor(CursorMove::Home),
        (Key::End, _) => Action::SqlModalMoveCursor(CursorMove::End),
        // Editing
        (Key::Backspace, _) => Action::SqlModalBackspace,
        (Key::Delete, _) => Action::SqlModalDelete,
        (Key::Enter, false) => Action::SqlModalNewLine,
        (Key::Tab, false) => Action::SqlModalTab,
        (Key::Char(c), _) => Action::SqlModalInput(c),
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::action::CursorMove;
    use crate::app::keybindings::{Key, KeyCombo};
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

    #[derive(Debug, PartialEq)]
    enum Expected {
        SqlModalSubmit,
        SqlModalNewLine,
        SqlModalTab,
        SqlModalBackspace,
        SqlModalDelete,
        SqlModalInput(char),
        SqlModalMoveCursor(CursorMove),
        CloseSqlModal,
        CompletionTrigger,
        CompletionAccept,
        CompletionDismiss,
        CompletionPrev,
        CompletionNext,
        SqlModalConfirmExecute,
        SqlModalCancelConfirm,
        None,
    }

    fn assert_action(result: Action, expected: Expected) {
        match expected {
            Expected::SqlModalSubmit => assert!(matches!(result, Action::SqlModalSubmit)),
            Expected::SqlModalNewLine => assert!(matches!(result, Action::SqlModalNewLine)),
            Expected::SqlModalTab => assert!(matches!(result, Action::SqlModalTab)),
            Expected::SqlModalBackspace => assert!(matches!(result, Action::SqlModalBackspace)),
            Expected::SqlModalDelete => assert!(matches!(result, Action::SqlModalDelete)),
            Expected::SqlModalInput(c) => {
                assert!(matches!(result, Action::SqlModalInput(x) if x == c))
            }
            Expected::SqlModalMoveCursor(m) => {
                assert!(matches!(result, Action::SqlModalMoveCursor(x) if x == m))
            }
            Expected::CloseSqlModal => assert!(matches!(result, Action::CloseSqlModal)),
            Expected::CompletionTrigger => assert!(matches!(result, Action::CompletionTrigger)),
            Expected::CompletionAccept => assert!(matches!(result, Action::CompletionAccept)),
            Expected::CompletionDismiss => assert!(matches!(result, Action::CompletionDismiss)),
            Expected::CompletionPrev => assert!(matches!(result, Action::CompletionPrev)),
            Expected::CompletionNext => assert!(matches!(result, Action::CompletionNext)),
            Expected::SqlModalConfirmExecute => {
                assert!(matches!(result, Action::SqlModalConfirmExecute))
            }
            Expected::SqlModalCancelConfirm => {
                assert!(matches!(result, Action::SqlModalCancelConfirm))
            }
            Expected::None => assert!(matches!(result, Action::None)),
        }
    }

    fn confirming_status() -> SqlModalStatus {
        use crate::app::write_guardrails::{AdhocRiskDecision, RiskLevel};
        SqlModalStatus::Confirming(AdhocRiskDecision {
            risk_level: RiskLevel::High,
            label: "DROP",
        })
    }

    #[rstest]
    #[case(Key::Enter, Expected::SqlModalConfirmExecute)]
    #[case(Key::Esc, Expected::SqlModalCancelConfirm)]
    fn confirming_state_routes_enter_and_esc(#[case] code: Key, #[case] expected: Expected) {
        let status = confirming_status();
        let result = handle_sql_modal_keys(combo(code), false, &status);

        assert_action(result, expected);
    }

    #[rstest]
    #[case(Key::Char('a'))]
    #[case(Key::Tab)]
    #[case(Key::Backspace)]
    fn confirming_state_ignores_editing_keys(#[case] code: Key) {
        let status = confirming_status();
        let result = handle_sql_modal_keys(combo(code), false, &status);

        assert_action(result, Expected::None);
    }

    #[test]
    fn confirming_state_ignores_alt_enter() {
        let status = confirming_status();
        let result = handle_sql_modal_keys(combo_alt(Key::Enter), false, &status);

        assert_action(result, Expected::None);
    }

    // Completion-aware keys: behavior when completion is hidden
    #[rstest]
    #[case(Key::Esc, Expected::CloseSqlModal)]
    #[case(Key::Tab, Expected::SqlModalTab)]
    #[case(Key::Enter, Expected::SqlModalNewLine)]
    #[case(Key::Up, Expected::SqlModalMoveCursor(CursorMove::Up))]
    #[case(Key::Down, Expected::SqlModalMoveCursor(CursorMove::Down))]
    fn completion_hidden_key_behavior(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(combo(code), false, &SqlModalStatus::Editing);

        assert_action(result, expected);
    }

    // Completion-aware keys: behavior when completion is visible
    #[rstest]
    #[case(Key::Esc, Expected::CompletionDismiss)]
    #[case(Key::Tab, Expected::CompletionAccept)]
    #[case(Key::Enter, Expected::CompletionAccept)]
    #[case(Key::Up, Expected::CompletionPrev)]
    #[case(Key::Down, Expected::CompletionNext)]
    fn completion_visible_key_behavior(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(combo(code), true, &SqlModalStatus::Editing);

        assert_action(result, expected);
    }

    // Keys unaffected by completion visibility
    #[rstest]
    #[case(Key::Backspace, Expected::SqlModalBackspace)]
    #[case(Key::Delete, Expected::SqlModalDelete)]
    #[case(Key::Left, Expected::SqlModalMoveCursor(CursorMove::Left))]
    #[case(Key::Right, Expected::SqlModalMoveCursor(CursorMove::Right))]
    #[case(Key::Home, Expected::SqlModalMoveCursor(CursorMove::Home))]
    #[case(Key::End, Expected::SqlModalMoveCursor(CursorMove::End))]
    #[case(Key::F(1), Expected::None)]
    fn completion_independent_keys(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(combo(code), false, &SqlModalStatus::Editing);

        assert_action(result, expected);
    }

    #[test]
    fn delete_key_returns_delete_action() {
        let result = handle_sql_modal_keys(combo(Key::Delete), false, &SqlModalStatus::Editing);

        assert_action(result, Expected::SqlModalDelete);
    }

    #[test]
    fn enter_without_completion_returns_newline() {
        let result = handle_sql_modal_keys(combo(Key::Enter), false, &SqlModalStatus::Editing);

        assert_action(result, Expected::SqlModalNewLine);
    }

    #[test]
    fn tab_without_completion_returns_tab() {
        let result = handle_sql_modal_keys(combo(Key::Tab), false, &SqlModalStatus::Editing);

        assert_action(result, Expected::SqlModalTab);
    }

    #[test]
    fn alt_enter_submits_query() {
        let result = handle_sql_modal_keys(combo_alt(Key::Enter), false, &SqlModalStatus::Editing);

        assert_action(result, Expected::SqlModalSubmit);
    }

    #[test]
    fn ctrl_space_triggers_completion() {
        let result =
            handle_sql_modal_keys(combo_ctrl(Key::Char(' ')), false, &SqlModalStatus::Editing);

        assert_action(result, Expected::CompletionTrigger);
    }

    #[rstest]
    #[case('a')]
    #[case('Z')]
    #[case('あ')]
    #[case('日')]
    fn char_input_inserts_character(#[case] c: char) {
        let result = handle_sql_modal_keys(combo(Key::Char(c)), false, &SqlModalStatus::Editing);

        assert_action(result, Expected::SqlModalInput(c));
    }
}
