use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::update::action::{
    Action, CursorPosition, ScrollAmount, ScrollDirection, ScrollTarget, ScrollToCursorTarget,
    SelectMotion,
};

use super::{scroll, scroll_to_cursor};
use crate::app::update::input::vim::types::{
    BrowseVimContext, InspectorVimContext, ResultVimContext, VimModeTransition, VimNavigation,
    VimOperator,
};

pub(in crate::app::update::input::vim) fn navigation(
    navigation: VimNavigation,
    ctx: BrowseVimContext,
) -> Action {
    match ctx {
        BrowseVimContext::Explorer => explorer_navigation(navigation),
        BrowseVimContext::Inspector(_) => inspector_navigation(navigation),
        BrowseVimContext::Result(result_ctx) => result_navigation(navigation, result_ctx),
    }
}

pub(in crate::app::update::input::vim) fn mode_transition(
    transition: VimModeTransition,
    ctx: BrowseVimContext,
) -> Action {
    match (transition, ctx) {
        (
            VimModeTransition::Escape,
            BrowseVimContext::Explorer | BrowseVimContext::Inspector(_),
        ) => Action::Escape,
        (VimModeTransition::Escape, BrowseVimContext::Result(result_ctx)) => {
            match result_ctx.mode {
                ResultNavMode::Scroll => Action::Escape,
                ResultNavMode::RowActive => Action::ResultExitToScroll,
                ResultNavMode::CellActive => {
                    if result_ctx.has_pending_draft {
                        Action::ResultDiscardCellEdit
                    } else {
                        Action::ResultExitToRowActive
                    }
                }
            }
        }
        (VimModeTransition::ConfirmOrEnter, BrowseVimContext::Explorer) => Action::ConfirmSelection,
        (VimModeTransition::ConfirmOrEnter, BrowseVimContext::Result(result_ctx)) => {
            match result_ctx.mode {
                ResultNavMode::Scroll => Action::ResultEnterRowActive,
                ResultNavMode::RowActive => Action::ResultEnterCellActive,
                ResultNavMode::CellActive => Action::None,
            }
        }
        (VimModeTransition::Insert, BrowseVimContext::Result(result_ctx))
            if result_ctx.mode == ResultNavMode::CellActive =>
        {
            Action::ResultEnterCellEdit
        }
        (VimModeTransition::ConfirmOrEnter, BrowseVimContext::Inspector(_))
        | (VimModeTransition::Insert, _) => Action::None,
    }
}

pub(in crate::app::update::input::vim) fn operator(
    operator: VimOperator,
    ctx: BrowseVimContext,
) -> Option<Action> {
    match (operator, ctx) {
        (VimOperator::Yank, BrowseVimContext::Inspector(InspectorVimContext::Ddl)) => {
            Some(Action::DdlYank)
        }
        (VimOperator::Yank, BrowseVimContext::Result(result_ctx)) => Some(match result_ctx.mode {
            ResultNavMode::Scroll => Action::None,
            ResultNavMode::RowActive => {
                if result_ctx.yank_pending {
                    Action::ResultRowYank
                } else {
                    Action::ResultRowYankOperatorPending
                }
            }
            ResultNavMode::CellActive => Action::ResultCellYank,
        }),
        (VimOperator::Delete, BrowseVimContext::Result(result_ctx))
            if result_ctx.mode == ResultNavMode::RowActive =>
        {
            Some(if result_ctx.delete_pending {
                Action::StageRowForDelete
            } else {
                Action::ResultDeleteOperatorPending
            })
        }
        _ => None,
    }
}

fn explorer_navigation(navigation: VimNavigation) -> Action {
    match navigation {
        VimNavigation::MoveDown => Action::Select(SelectMotion::Next),
        VimNavigation::MoveUp => Action::Select(SelectMotion::Previous),
        VimNavigation::MoveToFirst => Action::Select(SelectMotion::First),
        VimNavigation::MoveToLast => Action::Select(SelectMotion::Last),
        VimNavigation::ViewportTop => Action::Select(SelectMotion::ViewportTop),
        VimNavigation::ViewportMiddle => Action::Select(SelectMotion::ViewportMiddle),
        VimNavigation::ViewportBottom => Action::Select(SelectMotion::ViewportBottom),
        VimNavigation::MoveLeft => scroll(
            ScrollTarget::Explorer,
            ScrollDirection::Left,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveRight => scroll(
            ScrollTarget::Explorer,
            ScrollDirection::Right,
            ScrollAmount::Line,
        ),
        VimNavigation::HalfPageDown => Action::Select(SelectMotion::HalfPageDown),
        VimNavigation::HalfPageUp => Action::Select(SelectMotion::HalfPageUp),
        VimNavigation::FullPageDown => Action::Select(SelectMotion::FullPageDown),
        VimNavigation::FullPageUp => Action::Select(SelectMotion::FullPageUp),
        VimNavigation::ScrollCursorCenter => {
            scroll_to_cursor(ScrollToCursorTarget::Explorer, CursorPosition::Center)
        }
        VimNavigation::ScrollCursorTop => {
            scroll_to_cursor(ScrollToCursorTarget::Explorer, CursorPosition::Top)
        }
        VimNavigation::ScrollCursorBottom => {
            scroll_to_cursor(ScrollToCursorTarget::Explorer, CursorPosition::Bottom)
        }
    }
}

fn inspector_navigation(navigation: VimNavigation) -> Action {
    match navigation {
        VimNavigation::MoveDown => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Down,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveUp => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Up,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveToFirst => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Up,
            ScrollAmount::ToStart,
        ),
        VimNavigation::MoveToLast => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Down,
            ScrollAmount::ToEnd,
        ),
        VimNavigation::MoveLeft => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Left,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveRight => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Right,
            ScrollAmount::Line,
        ),
        VimNavigation::HalfPageDown => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Down,
            ScrollAmount::HalfPage,
        ),
        VimNavigation::HalfPageUp => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Up,
            ScrollAmount::HalfPage,
        ),
        VimNavigation::FullPageDown => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Down,
            ScrollAmount::FullPage,
        ),
        VimNavigation::FullPageUp => scroll(
            ScrollTarget::Inspector,
            ScrollDirection::Up,
            ScrollAmount::FullPage,
        ),
        VimNavigation::ViewportTop
        | VimNavigation::ViewportMiddle
        | VimNavigation::ViewportBottom
        | VimNavigation::ScrollCursorCenter
        | VimNavigation::ScrollCursorTop
        | VimNavigation::ScrollCursorBottom => Action::None,
    }
}

fn result_navigation(navigation: VimNavigation, ctx: ResultVimContext) -> Action {
    match navigation {
        VimNavigation::MoveDown => scroll(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveUp => scroll(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveToFirst => scroll(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::ToStart,
        ),
        VimNavigation::MoveToLast => scroll(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::ToEnd,
        ),
        VimNavigation::ViewportTop => scroll(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::ViewportTop,
        ),
        VimNavigation::ViewportMiddle => scroll(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::ViewportMiddle,
        ),
        VimNavigation::ViewportBottom => scroll(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::ViewportBottom,
        ),
        VimNavigation::MoveLeft => {
            if ctx.mode == ResultNavMode::CellActive {
                Action::ResultCellLeft
            } else {
                scroll(
                    ScrollTarget::Result,
                    ScrollDirection::Left,
                    ScrollAmount::Line,
                )
            }
        }
        VimNavigation::MoveRight => {
            if ctx.mode == ResultNavMode::CellActive {
                Action::ResultCellRight
            } else {
                scroll(
                    ScrollTarget::Result,
                    ScrollDirection::Right,
                    ScrollAmount::Line,
                )
            }
        }
        VimNavigation::HalfPageDown => scroll(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::HalfPage,
        ),
        VimNavigation::HalfPageUp => scroll(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::HalfPage,
        ),
        VimNavigation::FullPageDown => scroll(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::FullPage,
        ),
        VimNavigation::FullPageUp => scroll(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::FullPage,
        ),
        VimNavigation::ScrollCursorCenter => {
            scroll_to_cursor(ScrollToCursorTarget::Result, CursorPosition::Center)
        }
        VimNavigation::ScrollCursorTop => {
            scroll_to_cursor(ScrollToCursorTarget::Result, CursorPosition::Top)
        }
        VimNavigation::ScrollCursorBottom => {
            scroll_to_cursor(ScrollToCursorTarget::Result, CursorPosition::Bottom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::ui_state::ResultNavMode;
    use crate::app::update::input::vim::{VimCommand, VimSurfaceContext, action_for_command};
    use rstest::rstest;

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
            VimCommand::SearchContinuation(
                crate::app::update::input::vim::SearchContinuation::Next,
            ),
            browse_result(result_ctx(ResultNavMode::Scroll)),
        );

        assert!(action.is_none());
    }
}
