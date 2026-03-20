use crate::app::action::Action;
use crate::app::keybindings::{Key, KeyCombo};
use crate::app::sql_modal_context::{SqlModalStatus, SqlModalTab};

pub fn handle_sql_modal_keys(
    combo: KeyCombo,
    completion_visible: bool,
    status: &SqlModalStatus,
    active_tab: SqlModalTab,
) -> Action {
    use crate::app::action::CursorMove;

    // Running state: suppress all key input while EXPLAIN is executing
    if matches!(status, SqlModalStatus::Running) {
        return Action::None;
    }

    // Normal / Success / Error share the same command set (no text editing)
    if matches!(
        status,
        SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error
    ) {
        let ctrl = combo.modifiers.ctrl;
        let alt = combo.modifiers.alt;
        let plain = !ctrl && !alt;

        // EXPLAIN keys (available in both tabs)
        if ctrl && combo.key == Key::Char('e') {
            return Action::ExplainRequest;
        }
        if alt && combo.key == Key::Char('e') {
            return Action::ExplainAnalyzeRequest;
        }

        // Tab switching
        if plain && combo.key == Key::Tab {
            return Action::SqlModalNextTab;
        }
        if combo.key == Key::BackTab {
            return Action::SqlModalPrevTab;
        }

        // Plan tab specific keys (read-only viewer)
        if active_tab == SqlModalTab::Plan {
            return match combo.key {
                Key::Char('j') | Key::Down if plain => Action::ExplainPlanScrollDown,
                Key::Char('k') | Key::Up if plain => Action::ExplainPlanScrollUp,
                Key::Esc if plain => Action::CloseSqlModal,
                _ => Action::None,
            };
        }

        if ctrl && combo.key == Key::Char('o') {
            return Action::OpenQueryHistoryPicker;
        }
        if ctrl && combo.key == Key::Char('l') {
            return Action::SqlModalClear;
        }

        return match combo.key {
            Key::Enter if alt => Action::SqlModalSubmit,
            Key::Char('y') if plain => Action::SqlModalYank,
            Key::Enter if plain => Action::SqlModalEnterInsert,
            Key::Up => Action::SqlModalMoveCursor(CursorMove::Up),
            Key::Down => Action::SqlModalMoveCursor(CursorMove::Down),
            Key::Left => Action::SqlModalMoveCursor(CursorMove::Left),
            Key::Right => Action::SqlModalMoveCursor(CursorMove::Right),
            Key::Home => Action::SqlModalMoveCursor(CursorMove::Home),
            Key::End => Action::SqlModalMoveCursor(CursorMove::End),
            Key::Esc if plain => Action::CloseSqlModal,
            _ => Action::None,
        };
    }

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

    if ctrl && combo.key == Key::Char('o') {
        return Action::OpenQueryHistoryPicker;
    }

    if ctrl && combo.key == Key::Char(' ') {
        return Action::CompletionTrigger;
    }

    if ctrl && combo.key == Key::Char('e') {
        return Action::ExplainRequest;
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

        (Key::Esc, false) => Action::SqlModalEnterNormal,
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
        SqlModalEnterInsert,
        SqlModalEnterNormal,
        SqlModalYank,
        CompletionTrigger,
        CompletionAccept,
        CompletionDismiss,
        CompletionPrev,
        CompletionNext,
        SqlModalConfirmExecute,
        SqlModalCancelConfirm,
        OpenQueryHistoryPicker,
        SqlModalClear,
        ExplainRequest,
        ExplainAnalyzeRequest,
        SqlModalNextTab,
        SqlModalPrevTab,
        ExplainPlanScrollUp,
        ExplainPlanScrollDown,
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
            Expected::SqlModalEnterInsert => {
                assert!(matches!(result, Action::SqlModalEnterInsert))
            }
            Expected::SqlModalEnterNormal => {
                assert!(matches!(result, Action::SqlModalEnterNormal))
            }
            Expected::SqlModalYank => assert!(matches!(result, Action::SqlModalYank)),
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
            Expected::OpenQueryHistoryPicker => {
                assert!(matches!(result, Action::OpenQueryHistoryPicker))
            }
            Expected::SqlModalClear => assert!(matches!(result, Action::SqlModalClear)),
            Expected::ExplainRequest => assert!(matches!(result, Action::ExplainRequest)),
            Expected::ExplainAnalyzeRequest => {
                assert!(matches!(result, Action::ExplainAnalyzeRequest))
            }
            Expected::SqlModalNextTab => assert!(matches!(result, Action::SqlModalNextTab)),
            Expected::SqlModalPrevTab => assert!(matches!(result, Action::SqlModalPrevTab)),
            Expected::ExplainPlanScrollUp => {
                assert!(matches!(result, Action::ExplainPlanScrollUp))
            }
            Expected::ExplainPlanScrollDown => {
                assert!(matches!(result, Action::ExplainPlanScrollDown))
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
        let result = handle_sql_modal_keys(combo(code), false, &status, SqlModalTab::Sql);

        assert_action(result, expected);
    }

    #[rstest]
    #[case(Key::Char('a'))]
    #[case(Key::Tab)]
    #[case(Key::Backspace)]
    fn confirming_state_ignores_editing_keys(#[case] code: Key) {
        let status = confirming_status();
        let result = handle_sql_modal_keys(combo(code), false, &status, SqlModalTab::Sql);

        assert_action(result, Expected::None);
    }

    #[test]
    fn confirming_state_ignores_alt_enter() {
        let status = confirming_status();
        let result = handle_sql_modal_keys(combo_alt(Key::Enter), false, &status, SqlModalTab::Sql);

        assert_action(result, Expected::None);
    }

    // Completion-aware keys: behavior when completion is hidden
    #[rstest]
    #[case(Key::Esc, Expected::SqlModalEnterNormal)]
    #[case(Key::Tab, Expected::SqlModalTab)]
    #[case(Key::Enter, Expected::SqlModalNewLine)]
    #[case(Key::Up, Expected::SqlModalMoveCursor(CursorMove::Up))]
    #[case(Key::Down, Expected::SqlModalMoveCursor(CursorMove::Down))]
    fn completion_hidden_key_behavior(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

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
        let result = handle_sql_modal_keys(
            combo(code),
            true,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

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
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, expected);
    }

    #[test]
    fn delete_key_returns_delete_action() {
        let result = handle_sql_modal_keys(
            combo(Key::Delete),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalDelete);
    }

    #[test]
    fn enter_without_completion_returns_newline() {
        let result = handle_sql_modal_keys(
            combo(Key::Enter),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalNewLine);
    }

    #[test]
    fn tab_without_completion_returns_tab() {
        let result = handle_sql_modal_keys(
            combo(Key::Tab),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalTab);
    }

    #[test]
    fn alt_enter_submits_query() {
        let result = handle_sql_modal_keys(
            combo_alt(Key::Enter),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalSubmit);
    }

    #[test]
    fn ctrl_o_opens_query_history_picker() {
        let result = handle_sql_modal_keys(
            combo_ctrl(Key::Char('o')),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert!(matches!(result, Action::OpenQueryHistoryPicker));
    }

    #[test]
    fn ctrl_space_triggers_completion() {
        let result = handle_sql_modal_keys(
            combo_ctrl(Key::Char(' ')),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::CompletionTrigger);
    }

    #[rstest]
    #[case('a')]
    #[case('Z')]
    #[case('あ')]
    #[case('日')]
    fn char_input_inserts_character(#[case] c: char) {
        let result = handle_sql_modal_keys(
            combo(Key::Char(c)),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalInput(c));
    }

    #[rstest]
    #[case(Key::Char('y'), Expected::SqlModalYank)]
    #[case(Key::Enter, Expected::SqlModalEnterInsert)]
    #[case(Key::Esc, Expected::CloseSqlModal)]
    #[case(Key::Up, Expected::SqlModalMoveCursor(CursorMove::Up))]
    #[case(Key::Down, Expected::SqlModalMoveCursor(CursorMove::Down))]
    #[case(Key::Left, Expected::SqlModalMoveCursor(CursorMove::Left))]
    #[case(Key::Right, Expected::SqlModalMoveCursor(CursorMove::Right))]
    #[case(Key::Char('a'), Expected::None)]
    fn normal_mode_key_behavior(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, expected);
    }

    #[test]
    fn normal_mode_alt_enter_submits() {
        let result = handle_sql_modal_keys(
            combo_alt(Key::Enter),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalSubmit);
    }

    #[test]
    fn normal_mode_ctrl_o_opens_history() {
        let result = handle_sql_modal_keys(
            combo_ctrl(Key::Char('o')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::OpenQueryHistoryPicker);
    }

    #[test]
    fn normal_mode_ctrl_l_clears() {
        let result = handle_sql_modal_keys(
            combo_ctrl(Key::Char('l')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalClear);
    }

    #[rstest]
    #[case(SqlModalStatus::Success)]
    #[case(SqlModalStatus::Error)]
    fn success_error_share_normal_keybindings(#[case] status: SqlModalStatus) {
        let yank = handle_sql_modal_keys(combo(Key::Char('y')), false, &status, SqlModalTab::Sql);
        let enter = handle_sql_modal_keys(combo(Key::Enter), false, &status, SqlModalTab::Sql);
        let close = handle_sql_modal_keys(combo(Key::Esc), false, &status, SqlModalTab::Sql);

        assert_action(yank, Expected::SqlModalYank);
        assert_action(enter, Expected::SqlModalEnterInsert);
        assert_action(close, Expected::CloseSqlModal);
    }

    #[test]
    fn normal_mode_ctrl_e_requests_explain() {
        let result = handle_sql_modal_keys(
            combo_ctrl(Key::Char('e')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::ExplainRequest);
    }

    #[test]
    fn normal_mode_alt_e_requests_explain_analyze() {
        let result = handle_sql_modal_keys(
            combo_alt(Key::Char('e')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::ExplainAnalyzeRequest);
    }

    #[test]
    fn normal_mode_tab_switches_to_next_tab() {
        let result = handle_sql_modal_keys(
            combo(Key::Tab),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalNextTab);
    }

    #[test]
    fn normal_mode_backtab_switches_to_prev_tab() {
        let result = handle_sql_modal_keys(
            KeyCombo::plain(Key::BackTab),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalPrevTab);
    }

    #[test]
    fn editing_mode_ctrl_e_requests_explain() {
        let result = handle_sql_modal_keys(
            combo_ctrl(Key::Char('e')),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::ExplainRequest);
    }

    #[rstest]
    #[case(Key::Char('j'), Expected::ExplainPlanScrollDown)]
    #[case(Key::Down, Expected::ExplainPlanScrollDown)]
    #[case(Key::Char('k'), Expected::ExplainPlanScrollUp)]
    #[case(Key::Up, Expected::ExplainPlanScrollUp)]
    #[case(Key::Enter, Expected::None)]
    #[case(Key::Esc, Expected::CloseSqlModal)]
    #[case(Key::Char('a'), Expected::None)]
    fn plan_tab_key_behavior(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, expected);
    }

    #[test]
    fn plan_tab_ctrl_e_requests_explain() {
        let result = handle_sql_modal_keys(
            combo_ctrl(Key::Char('e')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, Expected::ExplainRequest);
    }

    #[rstest]
    #[case(Key::Char('a'))]
    #[case(Key::Enter)]
    #[case(Key::Esc)]
    #[case(Key::Tab)]
    #[case(Key::Up)]
    #[case(Key::Down)]
    fn running_state_suppresses_all_keys(#[case] code: Key) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Running,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::None);
    }

    #[test]
    fn plan_tab_alt_e_requests_explain_analyze() {
        let result = handle_sql_modal_keys(
            combo_alt(Key::Char('e')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, Expected::ExplainAnalyzeRequest);
    }
}
