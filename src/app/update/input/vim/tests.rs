use super::*;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::key_sequence::Prefix;
use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::update::action::{
    Action, CursorMove, CursorPosition, InputTarget, ScrollAmount, ScrollDirection, ScrollTarget,
    ScrollToCursorTarget,
};
use crate::app::update::input::keybindings::{Key, KeyCombo};
use rstest::rstest;

fn combo(key: Key) -> KeyCombo {
    KeyCombo::plain(key)
}

fn combo_ctrl(key: Key) -> KeyCombo {
    KeyCombo::ctrl(key)
}

fn result_ctx(mode: ResultNavMode) -> ResultVimContext {
    ResultVimContext {
        mode,
        has_pending_draft: false,
        yank_pending: false,
        delete_pending: false,
    }
}

fn browse_result(ctx: ResultVimContext) -> VimSurfaceContext {
    VimSurfaceContext::Browse(BrowseVimContext::Result(ctx))
}

mod classify {
    use super::*;

    #[rstest]
    #[case(Key::Char('i'), VimModeTransition::Insert)]
    #[case(Key::Enter, VimModeTransition::ConfirmOrEnter)]
    #[case(Key::Esc, VimModeTransition::Escape)]
    fn mode_transition_keys(#[case] key: Key, #[case] expected: VimModeTransition) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::ModeTransition(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('n'), SearchContinuation::Next)]
    #[case(Key::Char('N'), SearchContinuation::Prev)]
    fn search_keys(#[case] key: Key, #[case] expected: SearchContinuation) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::SearchContinuation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('y'), VimOperator::Yank)]
    #[case(Key::Char('d'), VimOperator::Delete)]
    fn operator_keys(#[case] key: Key, #[case] expected: VimOperator) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::Operator(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('j'), false, VimNavigation::MoveDown)]
    #[case(Key::Down, false, VimNavigation::MoveDown)]
    #[case(Key::Char('n'), true, VimNavigation::MoveDown)]
    #[case(Key::Char('k'), false, VimNavigation::MoveUp)]
    #[case(Key::Up, false, VimNavigation::MoveUp)]
    #[case(Key::Char('p'), true, VimNavigation::MoveUp)]
    fn vertical_navigation_aliases(
        #[case] key: Key,
        #[case] ctrl: bool,
        #[case] expected: VimNavigation,
    ) {
        let combo = if ctrl { combo_ctrl(key) } else { combo(key) };

        assert_eq!(
            classify_command(&combo),
            Some(VimCommand::Navigation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('h'), VimNavigation::MoveLeft)]
    #[case(Key::Left, VimNavigation::MoveLeft)]
    #[case(Key::Char('l'), VimNavigation::MoveRight)]
    #[case(Key::Right, VimNavigation::MoveRight)]
    fn horizontal_navigation_aliases(#[case] key: Key, #[case] expected: VimNavigation) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::Navigation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('g'), VimNavigation::MoveToFirst)]
    #[case(Key::Home, VimNavigation::MoveToFirst)]
    #[case(Key::Char('G'), VimNavigation::MoveToLast)]
    #[case(Key::End, VimNavigation::MoveToLast)]
    #[case(Key::Char('H'), VimNavigation::ViewportTop)]
    #[case(Key::Char('M'), VimNavigation::ViewportMiddle)]
    #[case(Key::Char('L'), VimNavigation::ViewportBottom)]
    fn boundary_navigation_aliases(#[case] key: Key, #[case] expected: VimNavigation) {
        assert_eq!(
            classify_command(&combo(key)),
            Some(VimCommand::Navigation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('d'), true, VimNavigation::HalfPageDown)]
    #[case(Key::Char('u'), true, VimNavigation::HalfPageUp)]
    #[case(Key::Char('f'), true, VimNavigation::FullPageDown)]
    #[case(Key::PageDown, false, VimNavigation::FullPageDown)]
    #[case(Key::Char('b'), true, VimNavigation::FullPageUp)]
    #[case(Key::PageUp, false, VimNavigation::FullPageUp)]
    fn paging_navigation_aliases(
        #[case] key: Key,
        #[case] ctrl: bool,
        #[case] expected: VimNavigation,
    ) {
        let combo = if ctrl { combo_ctrl(key) } else { combo(key) };

        assert_eq!(
            classify_command(&combo),
            Some(VimCommand::Navigation(expected))
        );
    }

    #[rstest]
    #[case(Key::Char('z'), VimNavigation::ScrollCursorCenter)]
    #[case(Key::Char('t'), VimNavigation::ScrollCursorTop)]
    #[case(Key::Char('b'), VimNavigation::ScrollCursorBottom)]
    fn z_sequence_navigation(#[case] key: Key, #[case] expected: VimNavigation) {
        assert_eq!(
            classify_sequence(Prefix::Z, &combo(key)),
            Some(VimCommand::Navigation(expected))
        );
    }
}

mod browse_context {
    use super::*;

    #[test]
    fn detects_result_pending_state() {
        let mut state = AppState::new("test".to_string());
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(0);
        state.result_interaction.yank_op_pending = true;
        state.result_interaction.delete_op_pending = true;

        let BrowseVimContext::Result(result_ctx) = BrowseVimContext::from_state(&state) else {
            panic!("expected result context");
        };

        assert_eq!(result_ctx.mode, ResultNavMode::CellActive);
        assert!(result_ctx.yank_pending);
        assert!(result_ctx.delete_pending);
    }
}

mod browse_resolve {
    use super::*;

    #[test]
    fn result_cell_escape_with_draft_discards_edit() {
        let action = action_for_command(
            VimCommand::ModeTransition(VimModeTransition::Escape),
            browse_result(ResultVimContext {
                has_pending_draft: true,
                ..result_ctx(ResultNavMode::CellActive)
            }),
        );

        assert!(matches!(action, Some(Action::ResultDiscardCellEdit)));
    }

    #[test]
    fn result_scroll_mode_move_down_resolves_to_result_line_scroll() {
        let action = action_for_command(
            VimCommand::Navigation(VimNavigation::MoveDown),
            browse_result(result_ctx(ResultNavMode::Scroll)),
        );

        assert!(matches!(
            action,
            Some(Action::Scroll {
                target: ScrollTarget::Result,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            })
        ));
    }

    #[test]
    fn explorer_scroll_cursor_center_resolves_to_scroll_to_cursor() {
        let action = action_for_command(
            VimCommand::Navigation(VimNavigation::ScrollCursorCenter),
            VimSurfaceContext::Browse(BrowseVimContext::Explorer),
        );

        assert!(matches!(
            action,
            Some(Action::ScrollToCursor {
                target: ScrollToCursorTarget::Explorer,
                position: CursorPosition::Center,
            })
        ));
    }

    #[test]
    fn inspector_viewport_navigation_resolves_to_none_action() {
        let action = action_for_command(
            VimCommand::Navigation(VimNavigation::ViewportTop),
            VimSurfaceContext::Browse(BrowseVimContext::Inspector(InspectorVimContext::Other)),
        );

        assert!(matches!(action, Some(Action::None)));
    }

    #[rstest]
    #[case(VimNavigation::MoveLeft, Action::ResultCellLeft)]
    #[case(VimNavigation::MoveRight, Action::ResultCellRight)]
    fn result_cell_left_right_use_cell_actions(
        #[case] navigation: VimNavigation,
        #[case] expected: Action,
    ) {
        let action = action_for_command(
            VimCommand::Navigation(navigation),
            browse_result(result_ctx(ResultNavMode::CellActive)),
        );

        assert!(matches!(
            (action, expected),
            (Some(Action::ResultCellLeft), Action::ResultCellLeft)
                | (Some(Action::ResultCellRight), Action::ResultCellRight)
        ));
    }

    #[test]
    fn result_row_yank_without_pending_sets_pending() {
        let action = action_for_command(
            VimCommand::Operator(VimOperator::Yank),
            browse_result(result_ctx(ResultNavMode::RowActive)),
        );

        assert!(matches!(action, Some(Action::ResultRowYankOperatorPending)));
    }

    #[test]
    fn result_row_yank_with_pending_executes_yank() {
        let action = action_for_command(
            VimCommand::Operator(VimOperator::Yank),
            browse_result(ResultVimContext {
                yank_pending: true,
                ..result_ctx(ResultNavMode::RowActive)
            }),
        );

        assert!(matches!(action, Some(Action::ResultRowYank)));
    }

    #[test]
    fn result_row_delete_without_pending_sets_pending() {
        let action = action_for_command(
            VimCommand::Operator(VimOperator::Delete),
            browse_result(result_ctx(ResultNavMode::RowActive)),
        );

        assert!(matches!(action, Some(Action::ResultDeleteOperatorPending)));
    }

    #[test]
    fn result_row_delete_with_pending_stages_delete() {
        let action = action_for_command(
            VimCommand::Operator(VimOperator::Delete),
            browse_result(ResultVimContext {
                delete_pending: true,
                ..result_ctx(ResultNavMode::RowActive)
            }),
        );

        assert!(matches!(action, Some(Action::StageRowForDelete)));
    }

    #[test]
    fn result_cell_yank_resolves_to_cell_yank() {
        let action = action_for_command(
            VimCommand::Operator(VimOperator::Yank),
            browse_result(result_ctx(ResultNavMode::CellActive)),
        );

        assert!(matches!(action, Some(Action::ResultCellYank)));
    }

    #[test]
    fn inspector_ddl_yank_resolves_to_ddl_yank() {
        let action = action_for_command(
            VimCommand::Operator(VimOperator::Yank),
            VimSurfaceContext::Browse(BrowseVimContext::Inspector(InspectorVimContext::Ddl)),
        );

        assert!(matches!(action, Some(Action::DdlYank)));
    }

    #[test]
    fn result_search_continuation_stays_unsupported() {
        let action = action_for_command(
            VimCommand::SearchContinuation(SearchContinuation::Next),
            browse_result(result_ctx(ResultNavMode::Scroll)),
        );

        assert!(action.is_none());
    }
}

mod sql_resolve {
    use super::*;

    #[rstest]
    #[case(Key::Char('i'))]
    #[case(Key::Enter)]
    fn insert_and_confirm_enter_insert(#[case] key: Key) {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal);

        let action = action_for_key(&combo(key), ctx);

        assert!(matches!(action, Some(Action::SqlModalEnterInsert)));
    }

    #[test]
    fn yank_copies_query() {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal);

        let action = action_for_key(&combo(Key::Char('y')), ctx);

        assert!(matches!(action, Some(Action::SqlModalYank)));
    }

    #[rstest]
    #[case(Key::Char('n'), ScrollDirection::Down)]
    #[case(Key::Char('p'), ScrollDirection::Up)]
    fn ctrl_aliases_scroll_by_line(#[case] key: Key, #[case] expected_direction: ScrollDirection) {
        let action = action_for_key(
            &combo_ctrl(key),
            VimSurfaceContext::SqlModal(SqlModalVimContext::PlanViewer),
        );

        assert!(matches!(
            action,
            Some(Action::Scroll {
                target: ScrollTarget::ExplainPlan,
                direction,
                amount: ScrollAmount::Line,
            }) if direction == expected_direction
        ));
    }
}

mod jsonb_resolve {
    use super::*;

    #[test]
    fn enter_opens_edit_mode() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Enter), ctx);

        assert!(matches!(action, Some(Action::JsonbEnterEdit)));
    }

    #[test]
    fn search_next_moves_to_match() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Char('n')), ctx);

        assert!(matches!(action, Some(Action::JsonbSearchNext)));
    }

    #[test]
    fn yank_copies_full_json() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Char('y')), ctx);

        assert!(matches!(action, Some(Action::JsonbYankAll)));
    }

    #[test]
    fn left_navigation_moves_text_cursor_left() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        let action = action_for_key(&combo(Key::Char('h')), ctx);

        assert!(matches!(
            action,
            Some(Action::TextMoveCursor {
                target: InputTarget::JsonbEdit,
                direction: CursorMove::Left,
            })
        ));
    }
}
