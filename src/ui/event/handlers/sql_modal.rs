use crate::app::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
use crate::app::update::action::{
    Action, InputTarget, ScrollAmount, ScrollDirection, ScrollTarget,
};
use crate::app::update::input::keybindings::{Key, KeyCombo};
use crate::app::update::input::vim::{SqlModalVimContext, VimSurfaceContext, action_for_key};

pub fn handle_sql_modal_keys(
    combo: KeyCombo,
    completion_visible: bool,
    status: &SqlModalStatus,
    active_tab: SqlModalTab,
) -> Action {
    use crate::app::update::action::CursorMove;

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

        if ctrl && combo.key == Key::Char('e') {
            return Action::ExplainRequest;
        }

        // Tab switching
        if plain && combo.key == Key::Tab {
            return Action::SqlModalNextTab;
        }
        if !combo.modifiers.ctrl && !combo.modifiers.alt && combo.key == Key::BackTab {
            return Action::SqlModalPrevTab;
        }

        // Plan tab specific keys (read-only viewer)
        if active_tab == SqlModalTab::Plan {
            if let Some(action) = action_for_key(
                &combo,
                VimSurfaceContext::SqlModal(SqlModalVimContext::PlanViewer),
            ) {
                return action;
            }

            return match combo.key {
                Key::Char('e') if alt => Action::ExplainAnalyzeRequest,
                _ => Action::None,
            };
        }

        // Compare tab specific keys (read-only viewer)
        if active_tab == SqlModalTab::Compare {
            if let Some(action) = action_for_key(
                &combo,
                VimSurfaceContext::SqlModal(SqlModalVimContext::CompareViewer),
            ) {
                return action;
            }

            return match combo.key {
                Key::Char('e') if alt => Action::ExplainAnalyzeRequest,
                Key::Char('e') if plain => Action::CompareEditQuery,
                _ => Action::None,
            };
        }

        if alt && combo.key == Key::Char('e') {
            return Action::ExplainAnalyzeRequest;
        }
        if ctrl && combo.key == Key::Char('o') {
            return Action::OpenQueryHistoryPicker;
        }
        if ctrl && combo.key == Key::Char('l') {
            return Action::SqlModalClear;
        }

        if let Some(action) = action_for_key(
            &combo,
            VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal),
        ) {
            return action;
        }

        return match combo.key {
            Key::Enter if alt => Action::SqlModalSubmit,
            Key::Up => Action::TextMoveCursor {
                target: InputTarget::SqlModal,
                direction: CursorMove::Up,
            },
            Key::Down => Action::TextMoveCursor {
                target: InputTarget::SqlModal,
                direction: CursorMove::Down,
            },
            Key::Left => Action::TextMoveCursor {
                target: InputTarget::SqlModal,
                direction: CursorMove::Left,
            },
            Key::Right => Action::TextMoveCursor {
                target: InputTarget::SqlModal,
                direction: CursorMove::Right,
            },
            Key::Home => Action::TextMoveCursor {
                target: InputTarget::SqlModal,
                direction: CursorMove::Home,
            },
            Key::End => Action::TextMoveCursor {
                target: InputTarget::SqlModal,
                direction: CursorMove::End,
            },
            _ => Action::None,
        };
    }

    if matches!(status, SqlModalStatus::ConfirmingHigh { .. }) {
        let plain = !combo.modifiers.ctrl && !combo.modifiers.alt;
        return match combo.key {
            Key::Char(c) if plain => Action::TextInput {
                target: InputTarget::SqlModalHighRisk,
                ch: c,
            },
            Key::Backspace if plain => Action::TextBackspace {
                target: InputTarget::SqlModalHighRisk,
            },
            Key::Left => Action::TextMoveCursor {
                target: InputTarget::SqlModalHighRisk,
                direction: CursorMove::Left,
            },
            Key::Right => Action::TextMoveCursor {
                target: InputTarget::SqlModalHighRisk,
                direction: CursorMove::Right,
            },
            Key::Home => Action::TextMoveCursor {
                target: InputTarget::SqlModalHighRisk,
                direction: CursorMove::Home,
            },
            Key::End => Action::TextMoveCursor {
                target: InputTarget::SqlModalHighRisk,
                direction: CursorMove::End,
            },
            Key::Enter if plain => Action::SqlModalHighRiskConfirmExecute,
            Key::Esc => Action::SqlModalCancelConfirm,
            _ => Action::None,
        };
    }

    if matches!(status, SqlModalStatus::ConfirmingAnalyzeHigh { .. }) {
        let plain = !combo.modifiers.ctrl && !combo.modifiers.alt;
        return match combo.key {
            Key::Up if plain => Action::Scroll {
                target: ScrollTarget::ExplainConfirm,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::Line,
            },
            Key::Down if plain => Action::Scroll {
                target: ScrollTarget::ExplainConfirm,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            },
            Key::Char(c) if plain => Action::TextInput {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                ch: c,
            },
            Key::Backspace if plain => Action::TextBackspace {
                target: InputTarget::SqlModalAnalyzeHighRisk,
            },
            Key::Left => Action::TextMoveCursor {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                direction: CursorMove::Left,
            },
            Key::Right => Action::TextMoveCursor {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                direction: CursorMove::Right,
            },
            Key::Home => Action::TextMoveCursor {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                direction: CursorMove::Home,
            },
            Key::End => Action::TextMoveCursor {
                target: InputTarget::SqlModalAnalyzeHighRisk,
                direction: CursorMove::End,
            },
            Key::Enter if plain => Action::ExplainAnalyzeConfirm,
            Key::Esc => Action::ExplainAnalyzeCancel,
            _ => Action::None,
        };
    }

    let ctrl = combo.modifiers.ctrl;
    let alt = combo.modifiers.alt;
    let shift = combo.modifiers.shift;
    let ctrl_only = ctrl && !alt && !shift;

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

    if alt && combo.key == Key::Char('e') {
        return Action::ExplainAnalyzeRequest;
    }

    if completion_visible {
        match combo.key {
            Key::Char('p') if ctrl_only => return Action::CompletionPrev,
            Key::Char('n') if ctrl_only => return Action::CompletionNext,
            Key::Up => return Action::CompletionPrev,
            Key::Down => return Action::CompletionNext,
            Key::Tab | Key::Enter => return Action::CompletionAccept,
            Key::Esc | Key::Left | Key::Right => return Action::CompletionDismiss,
            _ => {}
        }
    }

    if let Some(action) = action_for_key(
        &combo,
        VimSurfaceContext::SqlModal(SqlModalVimContext::QueryEditing),
    ) {
        return action;
    }

    match combo.key {
        Key::Left => Action::TextMoveCursor {
            target: InputTarget::SqlModal,
            direction: CursorMove::Left,
        },
        Key::Right => Action::TextMoveCursor {
            target: InputTarget::SqlModal,
            direction: CursorMove::Right,
        },
        Key::Up => Action::TextMoveCursor {
            target: InputTarget::SqlModal,
            direction: CursorMove::Up,
        },
        Key::Down => Action::TextMoveCursor {
            target: InputTarget::SqlModal,
            direction: CursorMove::Down,
        },
        Key::Home => Action::TextMoveCursor {
            target: InputTarget::SqlModal,
            direction: CursorMove::Home,
        },
        Key::End => Action::TextMoveCursor {
            target: InputTarget::SqlModal,
            direction: CursorMove::End,
        },
        // Editing
        Key::Backspace => Action::TextBackspace {
            target: InputTarget::SqlModal,
        },
        Key::Delete => Action::TextDelete {
            target: InputTarget::SqlModal,
        },
        Key::Enter => Action::SqlModalNewLine,
        Key::Tab => Action::SqlModalTab,
        Key::Char(c) => Action::TextInput {
            target: InputTarget::SqlModal,
            ch: c,
        },
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::action::CursorMove;
    use crate::app::update::input::keybindings::{Key, KeyCombo};
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
        OpenQueryHistoryPicker,
        SqlModalClear,
        ExplainRequest,
        ExplainAnalyzeRequest,
        SqlModalNextTab,
        SqlModalPrevTab,
        ExplainPlanScrollUp,
        ExplainPlanScrollDown,
        ExplainCompareScrollUp,
        ExplainCompareScrollDown,
        CompareEditQuery,
        None,
    }

    fn assert_action(result: Action, expected: Expected) {
        match expected {
            Expected::SqlModalSubmit => assert!(matches!(result, Action::SqlModalSubmit)),
            Expected::SqlModalNewLine => assert!(matches!(result, Action::SqlModalNewLine)),
            Expected::SqlModalTab => assert!(matches!(result, Action::SqlModalTab)),
            Expected::SqlModalBackspace => assert!(matches!(
                result,
                Action::TextBackspace {
                    target: InputTarget::SqlModal
                }
            )),
            Expected::SqlModalDelete => assert!(matches!(
                result,
                Action::TextDelete {
                    target: InputTarget::SqlModal
                }
            )),
            Expected::SqlModalInput(c) => {
                assert!(
                    matches!(result, Action::TextInput { target: InputTarget::SqlModal, ch: x } if x == c)
                );
            }
            Expected::SqlModalMoveCursor(m) => {
                assert!(
                    matches!(result, Action::TextMoveCursor { target: InputTarget::SqlModal, direction: x } if x == m)
                );
            }
            Expected::CloseSqlModal => assert!(matches!(result, Action::CloseSqlModal)),
            Expected::SqlModalEnterInsert => {
                assert!(matches!(result, Action::SqlModalEnterInsert));
            }
            Expected::SqlModalEnterNormal => {
                assert!(matches!(result, Action::SqlModalEnterNormal));
            }
            Expected::SqlModalYank => assert!(matches!(result, Action::SqlModalYank)),
            Expected::CompletionTrigger => assert!(matches!(result, Action::CompletionTrigger)),
            Expected::CompletionAccept => assert!(matches!(result, Action::CompletionAccept)),
            Expected::CompletionDismiss => assert!(matches!(result, Action::CompletionDismiss)),
            Expected::CompletionPrev => assert!(matches!(result, Action::CompletionPrev)),
            Expected::CompletionNext => assert!(matches!(result, Action::CompletionNext)),
            Expected::OpenQueryHistoryPicker => {
                assert!(matches!(result, Action::OpenQueryHistoryPicker));
            }
            Expected::SqlModalClear => assert!(matches!(result, Action::SqlModalClear)),
            Expected::ExplainRequest => assert!(matches!(result, Action::ExplainRequest)),
            Expected::ExplainAnalyzeRequest => {
                assert!(matches!(result, Action::ExplainAnalyzeRequest));
            }
            Expected::SqlModalNextTab => assert!(matches!(result, Action::SqlModalNextTab)),
            Expected::SqlModalPrevTab => assert!(matches!(result, Action::SqlModalPrevTab)),
            Expected::ExplainPlanScrollUp => {
                assert!(matches!(
                    result,
                    Action::Scroll {
                        target: ScrollTarget::ExplainPlan,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line
                    }
                ));
            }
            Expected::ExplainPlanScrollDown => {
                assert!(matches!(
                    result,
                    Action::Scroll {
                        target: ScrollTarget::ExplainPlan,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line
                    }
                ));
            }
            Expected::ExplainCompareScrollUp => {
                assert!(matches!(
                    result,
                    Action::Scroll {
                        target: ScrollTarget::ExplainCompare,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line
                    }
                ));
            }
            Expected::ExplainCompareScrollDown => {
                assert!(matches!(
                    result,
                    Action::Scroll {
                        target: ScrollTarget::ExplainCompare,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line
                    }
                ));
            }
            Expected::CompareEditQuery => {
                assert!(matches!(result, Action::CompareEditQuery));
            }
            Expected::None => assert!(matches!(result, Action::None)),
        }
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

    #[rstest]
    #[case(Key::Enter)]
    #[case(Key::Char('i'))]
    fn normal_sql_tab_accepts_shared_insert_keys(#[case] code: Key) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalEnterInsert);
    }

    #[rstest]
    #[case(Key::Char('p'), Expected::CompletionPrev)]
    #[case(Key::Char('n'), Expected::CompletionNext)]
    fn completion_visible_ctrl_aliases(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo_ctrl(code),
            true,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, expected);
    }

    #[rstest]
    #[case(Key::Char('p'))]
    #[case(Key::Char('n'))]
    fn ctrl_alt_aliases_fall_through_to_text_input(#[case] code: Key) {
        let result = handle_sql_modal_keys(
            KeyCombo::ctrl_alt(code),
            true,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );
        assert_action(
            result,
            Expected::SqlModalInput(match code {
                Key::Char(c) => c,
                _ => unreachable!(),
            }),
        );
    }

    #[rstest]
    #[case(Key::Char('p'))]
    #[case(Key::Char('n'))]
    fn ctrl_shift_aliases_fall_through_to_text_input(#[case] code: Key) {
        let result = handle_sql_modal_keys(
            KeyCombo::ctrl_shift(code),
            true,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );
        assert_action(
            result,
            Expected::SqlModalInput(match code {
                Key::Char(c) => c,
                _ => unreachable!(),
            }),
        );
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
    #[case(Key::Up, Expected::SqlModalMoveCursor(CursorMove::Up))]
    #[case(Key::Down, Expected::SqlModalMoveCursor(CursorMove::Down))]
    #[case(Key::Left, Expected::SqlModalMoveCursor(CursorMove::Left))]
    #[case(Key::Right, Expected::SqlModalMoveCursor(CursorMove::Right))]
    fn normal_mode_arrow_keys_move_cursor(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, expected);
    }

    #[rstest]
    #[case(Key::Char('n'), Expected::ExplainPlanScrollDown)]
    #[case(Key::Char('p'), Expected::ExplainPlanScrollUp)]
    fn plan_tab_ctrl_aliases_scroll(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo_ctrl(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, expected);
    }

    #[rstest]
    #[case(Key::Char('n'), Expected::ExplainCompareScrollDown)]
    #[case(Key::Char('p'), Expected::ExplainCompareScrollUp)]
    fn compare_tab_ctrl_aliases_scroll(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo_ctrl(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, expected);
    }

    #[rstest]
    #[case(SqlModalTab::Plan, Key::Char('n'))]
    #[case(SqlModalTab::Plan, Key::Char('p'))]
    #[case(SqlModalTab::Compare, Key::Char('n'))]
    #[case(SqlModalTab::Compare, Key::Char('p'))]
    fn ctrl_alt_aliases_do_not_scroll_in_read_only_tabs(
        #[case] tab: SqlModalTab,
        #[case] code: Key,
    ) {
        let result = handle_sql_modal_keys(
            KeyCombo::ctrl_alt(code),
            false,
            &SqlModalStatus::Normal,
            tab,
        );

        assert_action(result, Expected::None);
    }

    #[rstest]
    #[case(SqlModalTab::Plan, Key::Char('n'))]
    #[case(SqlModalTab::Plan, Key::Char('p'))]
    #[case(SqlModalTab::Compare, Key::Char('n'))]
    #[case(SqlModalTab::Compare, Key::Char('p'))]
    fn ctrl_shift_aliases_do_not_scroll_in_read_only_tabs(
        #[case] tab: SqlModalTab,
        #[case] code: Key,
    ) {
        let result = handle_sql_modal_keys(
            KeyCombo::ctrl_shift(code),
            false,
            &SqlModalStatus::Normal,
            tab,
        );

        assert_action(result, Expected::None);
    }

    #[test]
    fn normal_mode_y_yanks_query() {
        let result = handle_sql_modal_keys(
            combo(Key::Char('y')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalYank);
    }

    #[test]
    fn normal_mode_enter_enters_insert() {
        let result = handle_sql_modal_keys(
            combo(Key::Enter),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::SqlModalEnterInsert);
    }

    #[test]
    fn normal_mode_esc_closes_modal() {
        let result = handle_sql_modal_keys(
            combo(Key::Esc),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::CloseSqlModal);
    }

    #[test]
    fn normal_mode_unbound_keys_returns_none() {
        let result = handle_sql_modal_keys(
            combo(Key::Char('a')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::None);
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
    fn plan_tab_jk_and_arrow_keys_scroll_plan(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, expected);
    }

    #[test]
    fn plan_tab_y_returns_sql_modal_yank() {
        let result = handle_sql_modal_keys(
            combo(Key::Char('y')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, Expected::SqlModalYank);
    }

    #[test]
    fn compare_tab_y_returns_sql_modal_yank() {
        let result = handle_sql_modal_keys(
            combo(Key::Char('y')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::SqlModalYank);
    }

    #[test]
    fn plan_tab_esc_closes_modal() {
        let result = handle_sql_modal_keys(
            combo(Key::Esc),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, Expected::CloseSqlModal);
    }

    #[rstest]
    #[case(Key::Enter)]
    #[case(Key::Char('a'))]
    fn plan_tab_unbound_keys_returns_none(#[case] code: Key) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, Expected::None);
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

    #[rstest]
    #[case(Key::Char('j'), Expected::ExplainCompareScrollDown)]
    #[case(Key::Down, Expected::ExplainCompareScrollDown)]
    #[case(Key::Char('k'), Expected::ExplainCompareScrollUp)]
    #[case(Key::Up, Expected::ExplainCompareScrollUp)]
    fn compare_tab_scroll_keys_scroll_comparison(#[case] code: Key, #[case] expected: Expected) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, expected);
    }

    #[test]
    fn compare_tab_esc_closes_modal() {
        let result = handle_sql_modal_keys(
            combo(Key::Esc),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::CloseSqlModal);
    }

    #[rstest]
    #[case(Key::Char('a'))]
    #[case(Key::Enter)]
    fn compare_tab_unbound_keys_returns_none(#[case] code: Key) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::None);
    }

    #[test]
    fn compare_tab_ctrl_e_requests_explain() {
        let result = handle_sql_modal_keys(
            combo_ctrl(Key::Char('e')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::ExplainRequest);
    }

    #[test]
    fn compare_tab_alt_e_requests_analyze() {
        let result = handle_sql_modal_keys(
            combo_alt(Key::Char('e')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::ExplainAnalyzeRequest);
    }

    #[test]
    fn compare_tab_e_edits_query() {
        let result = handle_sql_modal_keys(
            combo(Key::Char('e')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::CompareEditQuery);
    }

    #[test]
    fn editing_alt_e_requests_analyze() {
        let result = handle_sql_modal_keys(
            combo_alt(Key::Char('e')),
            false,
            &SqlModalStatus::Editing,
            SqlModalTab::Sql,
        );

        assert_action(result, Expected::ExplainAnalyzeRequest);
    }

    #[test]
    fn compare_tab_tab_switches() {
        let result = handle_sql_modal_keys(
            combo(Key::Tab),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::SqlModalNextTab);
    }

    #[test]
    fn compare_tab_backtab_switches() {
        let result = handle_sql_modal_keys(
            KeyCombo::plain(Key::BackTab),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::SqlModalPrevTab);
    }

    #[rstest]
    #[case(Key::Char('a'))]
    #[case(Key::Enter)]
    #[case(Key::Esc)]
    #[case(Key::Tab)]
    #[case(Key::Up)]
    #[case(Key::Down)]
    fn running_state_compare_tab_suppresses_all_keys(#[case] code: Key) {
        let result = handle_sql_modal_keys(
            combo(code),
            false,
            &SqlModalStatus::Running,
            SqlModalTab::Compare,
        );

        assert_action(result, Expected::None);
    }

    #[test]
    fn plan_tab_alt_e_requests_analyze() {
        let result = handle_sql_modal_keys(
            combo_alt(Key::Char('e')),
            false,
            &SqlModalStatus::Normal,
            SqlModalTab::Plan,
        );

        assert_action(result, Expected::ExplainAnalyzeRequest);
    }

    #[rstest]
    #[case(SqlModalStatus::Success)]
    #[case(SqlModalStatus::Error)]
    fn plan_tab_read_only_keys_work_in_success_error(#[case] status: SqlModalStatus) {
        let scroll =
            handle_sql_modal_keys(combo(Key::Char('j')), false, &status, SqlModalTab::Plan);
        let close = handle_sql_modal_keys(combo(Key::Esc), false, &status, SqlModalTab::Plan);

        assert_action(scroll, Expected::ExplainPlanScrollDown);
        assert_action(close, Expected::CloseSqlModal);
    }

    #[rstest]
    #[case(SqlModalStatus::Success)]
    #[case(SqlModalStatus::Error)]
    fn compare_tab_read_only_keys_work_in_success_error(#[case] status: SqlModalStatus) {
        let scroll =
            handle_sql_modal_keys(combo(Key::Char('j')), false, &status, SqlModalTab::Compare);
        let close = handle_sql_modal_keys(combo(Key::Esc), false, &status, SqlModalTab::Compare);
        let explain = handle_sql_modal_keys(
            combo_ctrl(Key::Char('e')),
            false,
            &status,
            SqlModalTab::Compare,
        );

        assert_action(scroll, Expected::ExplainCompareScrollDown);
        assert_action(close, Expected::CloseSqlModal);
        assert_action(explain, Expected::ExplainRequest);
    }

    // ================================================================
    // Contract tests: keybinding definitions ↔ handler consistency
    // ================================================================

    use crate::app::update::input::keybindings::{
        KeyBinding, SQL_MODAL_COMPARE_KEYS, SQL_MODAL_PLAN_KEYS,
    };

    fn assert_keybindings_match_handler(keys: &[KeyBinding], tab: SqlModalTab, label: &str) {
        for kb in keys {
            if matches!(kb.action, Action::None) || kb.combos.is_empty() {
                continue;
            }
            for c in kb.combos {
                let result = handle_sql_modal_keys(*c, false, &SqlModalStatus::Normal, tab);
                assert_eq!(
                    std::mem::discriminant(&result),
                    std::mem::discriminant(&kb.action),
                    "{label}: combo {:?} returned {:?}, expected {:?}",
                    c,
                    result,
                    kb.action,
                );
            }
        }
    }

    #[test]
    fn plan_keybinding_combo_returns_declared_action() {
        assert_keybindings_match_handler(SQL_MODAL_PLAN_KEYS, SqlModalTab::Plan, "PLAN");
    }

    #[test]
    fn compare_keybinding_combo_returns_declared_action() {
        assert_keybindings_match_handler(SQL_MODAL_COMPARE_KEYS, SqlModalTab::Compare, "COMPARE");
    }
}
