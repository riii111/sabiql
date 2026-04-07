use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::key_sequence::Prefix;
use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::update::action::{
    Action, CursorMove, CursorPosition, InputTarget, ScrollAmount, ScrollDirection, ScrollTarget,
    ScrollToCursorTarget, SelectMotion,
};
use crate::app::update::input::keybindings::{Key, KeyCombo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimCommand {
    Navigation(VimNavigation),
    ModeTransition(VimModeTransition),
    SearchContinuation(SearchContinuation),
    Operator(VimOperator),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimNavigation {
    MoveDown,
    MoveUp,
    MoveToFirst,
    MoveToLast,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
    MoveLeft,
    MoveRight,
    HalfPageDown,
    HalfPageUp,
    FullPageDown,
    FullPageUp,
    ScrollCursorCenter,
    ScrollCursorTop,
    ScrollCursorBottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimModeTransition {
    Escape,
    Insert,
    ConfirmOrEnter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchContinuation {
    Next,
    Prev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimOperator {
    Yank,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimSurfaceContext {
    Browse(BrowseVimContext),
    SqlModal(SqlModalVimContext),
    JsonbDetail(JsonbDetailVimContext),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseVimContext {
    Explorer,
    Inspector(InspectorVimContext),
    Result(ResultVimContext),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorVimContext {
    Ddl,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultVimContext {
    pub mode: ResultNavMode,
    pub has_pending_draft: bool,
    pub yank_pending: bool,
    pub delete_pending: bool,
}

impl BrowseVimContext {
    pub fn from_state(state: &AppState) -> Self {
        let result_nav = state.ui.is_focus_mode() || state.ui.focused_pane == FocusedPane::Result;

        if result_nav {
            return Self::Result(ResultVimContext {
                mode: state.result_interaction.selection().mode(),
                has_pending_draft: state.result_interaction.cell_edit().has_pending_draft(),
                yank_pending: state.result_interaction.yank_op_pending,
                delete_pending: state.result_interaction.delete_op_pending,
            });
        }

        if state.ui.focused_pane == FocusedPane::Inspector {
            let inspector_ctx = if state.ui.inspector_tab == InspectorTab::Ddl {
                InspectorVimContext::Ddl
            } else {
                InspectorVimContext::Other
            };
            Self::Inspector(inspector_ctx)
        } else {
            Self::Explorer
        }
    }

    pub fn is_result(self) -> bool {
        matches!(self, Self::Result(_))
    }

    pub fn is_inspector(self) -> bool {
        matches!(self, Self::Inspector(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlModalVimContext {
    QueryNormal,
    QueryEditing,
    PlanViewer,
    CompareViewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonbDetailVimContext {
    Viewing,
    Editing,
    Searching,
}

pub fn classify_command(combo: &KeyCombo) -> Option<VimCommand> {
    if combo.modifiers.alt {
        return None;
    }

    if let Some(navigation) = classify_navigation(combo) {
        return Some(VimCommand::Navigation(navigation));
    }

    if combo.modifiers.ctrl {
        return None;
    }

    match combo.key {
        Key::Esc => Some(VimCommand::ModeTransition(VimModeTransition::Escape)),
        Key::Enter => Some(VimCommand::ModeTransition(
            VimModeTransition::ConfirmOrEnter,
        )),
        Key::Char('i') => Some(VimCommand::ModeTransition(VimModeTransition::Insert)),
        Key::Char('n') => Some(VimCommand::SearchContinuation(SearchContinuation::Next)),
        Key::Char('N') => Some(VimCommand::SearchContinuation(SearchContinuation::Prev)),
        Key::Char('y') => Some(VimCommand::Operator(VimOperator::Yank)),
        Key::Char('d') => Some(VimCommand::Operator(VimOperator::Delete)),
        _ => None,
    }
}

pub fn classify_sequence_command(prefix: Prefix, combo: &KeyCombo) -> Option<VimCommand> {
    if combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.shift {
        return None;
    }

    match prefix {
        Prefix::Z => match combo.key {
            Key::Char('z') => Some(VimCommand::Navigation(VimNavigation::ScrollCursorCenter)),
            Key::Char('t') => Some(VimCommand::Navigation(VimNavigation::ScrollCursorTop)),
            Key::Char('b') => Some(VimCommand::Navigation(VimNavigation::ScrollCursorBottom)),
            _ => None,
        },
    }
}

pub fn resolve_key_input(
    combo: &KeyCombo,
    pending_prefix: Option<Prefix>,
    ctx: VimSurfaceContext,
) -> Option<Action> {
    let command = if let Some(prefix) = pending_prefix {
        classify_sequence_command(prefix, combo)?
    } else {
        classify_command(combo)?
    };

    resolve(command, ctx)
}

pub fn resolve_command(combo: &KeyCombo, ctx: VimSurfaceContext) -> Option<Action> {
    resolve_key_input(combo, None, ctx)
}

pub fn resolve(command: VimCommand, ctx: VimSurfaceContext) -> Option<Action> {
    match ctx {
        VimSurfaceContext::Browse(ctx) => resolve_browse(command, ctx),
        VimSurfaceContext::SqlModal(ctx) => resolve_sql_modal(command, ctx),
        VimSurfaceContext::JsonbDetail(ctx) => resolve_jsonb_detail(command, ctx),
    }
}

fn classify_navigation(combo: &KeyCombo) -> Option<VimNavigation> {
    if combo.modifiers.shift || combo.modifiers.alt {
        return None;
    }

    if combo.modifiers.ctrl {
        return match combo.key {
            Key::Char('n') => Some(VimNavigation::MoveDown),
            Key::Char('p') => Some(VimNavigation::MoveUp),
            Key::Char('d') => Some(VimNavigation::HalfPageDown),
            Key::Char('u') => Some(VimNavigation::HalfPageUp),
            Key::Char('f') => Some(VimNavigation::FullPageDown),
            Key::Char('b') => Some(VimNavigation::FullPageUp),
            _ => None,
        };
    }

    match combo.key {
        Key::Char('j') | Key::Down => Some(VimNavigation::MoveDown),
        Key::Char('k') | Key::Up => Some(VimNavigation::MoveUp),
        Key::Char('g') | Key::Home => Some(VimNavigation::MoveToFirst),
        Key::Char('G') | Key::End => Some(VimNavigation::MoveToLast),
        Key::Char('H') => Some(VimNavigation::ViewportTop),
        Key::Char('M') => Some(VimNavigation::ViewportMiddle),
        Key::Char('L') => Some(VimNavigation::ViewportBottom),
        Key::Char('h') | Key::Left => Some(VimNavigation::MoveLeft),
        Key::Char('l') | Key::Right => Some(VimNavigation::MoveRight),
        Key::PageDown => Some(VimNavigation::FullPageDown),
        Key::PageUp => Some(VimNavigation::FullPageUp),
        _ => None,
    }
}

fn resolve_browse(command: VimCommand, ctx: BrowseVimContext) -> Option<Action> {
    match command {
        VimCommand::Navigation(navigation) => Some(resolve_browse_navigation(navigation, ctx)),
        VimCommand::ModeTransition(transition) => {
            Some(resolve_browse_mode_transition(transition, ctx))
        }
        VimCommand::SearchContinuation(_) => None,
        VimCommand::Operator(operator) => resolve_browse_operator(operator, ctx),
    }
}

fn resolve_browse_navigation(navigation: VimNavigation, ctx: BrowseVimContext) -> Action {
    match ctx {
        BrowseVimContext::Explorer => resolve_explorer_navigation(navigation),
        BrowseVimContext::Inspector(_) => resolve_inspector_navigation(navigation),
        BrowseVimContext::Result(result_ctx) => resolve_result_navigation(navigation, result_ctx),
    }
}

fn resolve_explorer_navigation(navigation: VimNavigation) -> Action {
    match navigation {
        VimNavigation::MoveDown => Action::Select(SelectMotion::Next),
        VimNavigation::MoveUp => Action::Select(SelectMotion::Previous),
        VimNavigation::MoveToFirst => Action::Select(SelectMotion::First),
        VimNavigation::MoveToLast => Action::Select(SelectMotion::Last),
        VimNavigation::ViewportTop => Action::Select(SelectMotion::ViewportTop),
        VimNavigation::ViewportMiddle => Action::Select(SelectMotion::ViewportMiddle),
        VimNavigation::ViewportBottom => Action::Select(SelectMotion::ViewportBottom),
        VimNavigation::MoveLeft => scroll_action(
            ScrollTarget::Explorer,
            ScrollDirection::Left,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveRight => scroll_action(
            ScrollTarget::Explorer,
            ScrollDirection::Right,
            ScrollAmount::Line,
        ),
        VimNavigation::HalfPageDown => Action::Select(SelectMotion::HalfPageDown),
        VimNavigation::HalfPageUp => Action::Select(SelectMotion::HalfPageUp),
        VimNavigation::FullPageDown => Action::Select(SelectMotion::FullPageDown),
        VimNavigation::FullPageUp => Action::Select(SelectMotion::FullPageUp),
        VimNavigation::ScrollCursorCenter => {
            scroll_to_cursor_action(ScrollToCursorTarget::Explorer, CursorPosition::Center)
        }
        VimNavigation::ScrollCursorTop => {
            scroll_to_cursor_action(ScrollToCursorTarget::Explorer, CursorPosition::Top)
        }
        VimNavigation::ScrollCursorBottom => {
            scroll_to_cursor_action(ScrollToCursorTarget::Explorer, CursorPosition::Bottom)
        }
    }
}

fn resolve_inspector_navigation(navigation: VimNavigation) -> Action {
    match navigation {
        VimNavigation::MoveDown => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Down,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveUp => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Up,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveToFirst => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Up,
            ScrollAmount::ToStart,
        ),
        VimNavigation::MoveToLast => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Down,
            ScrollAmount::ToEnd,
        ),
        VimNavigation::MoveLeft => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Left,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveRight => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Right,
            ScrollAmount::Line,
        ),
        VimNavigation::HalfPageDown => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Down,
            ScrollAmount::HalfPage,
        ),
        VimNavigation::HalfPageUp => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Up,
            ScrollAmount::HalfPage,
        ),
        VimNavigation::FullPageDown => scroll_action(
            ScrollTarget::Inspector,
            ScrollDirection::Down,
            ScrollAmount::FullPage,
        ),
        VimNavigation::FullPageUp => scroll_action(
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

fn resolve_result_navigation(navigation: VimNavigation, ctx: ResultVimContext) -> Action {
    match navigation {
        VimNavigation::MoveDown => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveUp => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::Line,
        ),
        VimNavigation::MoveToFirst => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::ToStart,
        ),
        VimNavigation::MoveToLast => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::ToEnd,
        ),
        VimNavigation::ViewportTop => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::ViewportTop,
        ),
        VimNavigation::ViewportMiddle => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::ViewportMiddle,
        ),
        VimNavigation::ViewportBottom => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::ViewportBottom,
        ),
        VimNavigation::MoveLeft => {
            if ctx.mode == ResultNavMode::CellActive {
                Action::ResultCellLeft
            } else {
                scroll_action(
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
                scroll_action(
                    ScrollTarget::Result,
                    ScrollDirection::Right,
                    ScrollAmount::Line,
                )
            }
        }
        VimNavigation::HalfPageDown => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::HalfPage,
        ),
        VimNavigation::HalfPageUp => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::HalfPage,
        ),
        VimNavigation::FullPageDown => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Down,
            ScrollAmount::FullPage,
        ),
        VimNavigation::FullPageUp => scroll_action(
            ScrollTarget::Result,
            ScrollDirection::Up,
            ScrollAmount::FullPage,
        ),
        VimNavigation::ScrollCursorCenter => {
            scroll_to_cursor_action(ScrollToCursorTarget::Result, CursorPosition::Center)
        }
        VimNavigation::ScrollCursorTop => {
            scroll_to_cursor_action(ScrollToCursorTarget::Result, CursorPosition::Top)
        }
        VimNavigation::ScrollCursorBottom => {
            scroll_to_cursor_action(ScrollToCursorTarget::Result, CursorPosition::Bottom)
        }
    }
}

fn resolve_browse_mode_transition(transition: VimModeTransition, ctx: BrowseVimContext) -> Action {
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

fn resolve_browse_operator(operator: VimOperator, ctx: BrowseVimContext) -> Option<Action> {
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

fn resolve_sql_modal(command: VimCommand, ctx: SqlModalVimContext) -> Option<Action> {
    match ctx {
        SqlModalVimContext::QueryNormal => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseSqlModal),
            VimCommand::ModeTransition(
                VimModeTransition::Insert | VimModeTransition::ConfirmOrEnter,
            ) => Some(Action::SqlModalEnterInsert),
            VimCommand::Operator(VimOperator::Yank) => Some(Action::SqlModalYank),
            _ => None,
        },
        SqlModalVimContext::QueryEditing => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => {
                Some(Action::SqlModalEnterNormal)
            }
            _ => None,
        },
        SqlModalVimContext::PlanViewer => resolve_sql_viewer(command, ScrollTarget::ExplainPlan),
        SqlModalVimContext::CompareViewer => {
            resolve_sql_viewer(command, ScrollTarget::ExplainCompare)
        }
    }
}

fn resolve_sql_viewer(command: VimCommand, target: ScrollTarget) -> Option<Action> {
    match command {
        VimCommand::Navigation(VimNavigation::MoveDown) => Some(scroll_action(
            target,
            ScrollDirection::Down,
            ScrollAmount::Line,
        )),
        VimCommand::Navigation(VimNavigation::MoveUp) => Some(scroll_action(
            target,
            ScrollDirection::Up,
            ScrollAmount::Line,
        )),
        VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseSqlModal),
        VimCommand::Operator(VimOperator::Yank) => Some(Action::SqlModalYank),
        _ => None,
    }
}

fn resolve_jsonb_detail(command: VimCommand, ctx: JsonbDetailVimContext) -> Option<Action> {
    match ctx {
        JsonbDetailVimContext::Viewing => match command {
            VimCommand::Navigation(navigation) => jsonb_navigation_action(navigation),
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseJsonbDetail),
            VimCommand::ModeTransition(
                VimModeTransition::Insert | VimModeTransition::ConfirmOrEnter,
            ) => Some(Action::JsonbEnterEdit),
            VimCommand::SearchContinuation(SearchContinuation::Next) => {
                Some(Action::JsonbSearchNext)
            }
            VimCommand::SearchContinuation(SearchContinuation::Prev) => {
                Some(Action::JsonbSearchPrev)
            }
            VimCommand::Operator(VimOperator::Yank) => Some(Action::JsonbYankAll),
            VimCommand::Operator(VimOperator::Delete) => None,
        },
        JsonbDetailVimContext::Editing => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::JsonbExitEdit),
            _ => None,
        },
        JsonbDetailVimContext::Searching => None,
    }
}

fn jsonb_navigation_action(navigation: VimNavigation) -> Option<Action> {
    let direction = match navigation {
        VimNavigation::MoveLeft => CursorMove::Left,
        VimNavigation::MoveRight => CursorMove::Right,
        VimNavigation::MoveUp => CursorMove::Up,
        VimNavigation::MoveDown => CursorMove::Down,
        VimNavigation::MoveToFirst => CursorMove::Home,
        VimNavigation::MoveToLast => CursorMove::End,
        _ => return None,
    };

    Some(Action::TextMoveCursor {
        target: InputTarget::JsonbEdit,
        direction,
    })
}

fn scroll_action(target: ScrollTarget, direction: ScrollDirection, amount: ScrollAmount) -> Action {
    Action::Scroll {
        target,
        direction,
        amount,
    }
}

fn scroll_to_cursor_action(target: ScrollToCursorTarget, position: CursorPosition) -> Action {
    Action::ScrollToCursor { target, position }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::focused_pane::FocusedPane;

    fn combo(key: Key) -> KeyCombo {
        KeyCombo::plain(key)
    }

    fn combo_ctrl(key: Key) -> KeyCombo {
        KeyCombo::ctrl(key)
    }

    #[test]
    fn classify_mode_transition_keys() {
        assert_eq!(
            classify_command(&combo(Key::Char('i'))),
            Some(VimCommand::ModeTransition(VimModeTransition::Insert))
        );
        assert_eq!(
            classify_command(&combo(Key::Enter)),
            Some(VimCommand::ModeTransition(
                VimModeTransition::ConfirmOrEnter
            ))
        );
        assert_eq!(
            classify_command(&combo(Key::Esc)),
            Some(VimCommand::ModeTransition(VimModeTransition::Escape))
        );
    }

    #[test]
    fn classify_search_and_operator_keys() {
        assert_eq!(
            classify_command(&combo(Key::Char('n'))),
            Some(VimCommand::SearchContinuation(SearchContinuation::Next))
        );
        assert_eq!(
            classify_command(&combo(Key::Char('N'))),
            Some(VimCommand::SearchContinuation(SearchContinuation::Prev))
        );
        assert_eq!(
            classify_command(&combo(Key::Char('y'))),
            Some(VimCommand::Operator(VimOperator::Yank))
        );
        assert_eq!(
            classify_command(&combo(Key::Char('d'))),
            Some(VimCommand::Operator(VimOperator::Delete))
        );
    }

    #[test]
    fn classify_navigation_aliases() {
        assert_eq!(
            classify_command(&combo_ctrl(Key::Char('n'))),
            Some(VimCommand::Navigation(VimNavigation::MoveDown))
        );
        assert_eq!(
            classify_command(&combo(Key::Char('h'))),
            Some(VimCommand::Navigation(VimNavigation::MoveLeft))
        );
    }

    #[test]
    fn classify_z_sequence_as_navigation() {
        assert_eq!(
            classify_sequence_command(Prefix::Z, &combo(Key::Char('z'))),
            Some(VimCommand::Navigation(VimNavigation::ScrollCursorCenter))
        );
        assert_eq!(
            classify_sequence_command(Prefix::Z, &combo(Key::Char('t'))),
            Some(VimCommand::Navigation(VimNavigation::ScrollCursorTop))
        );
    }

    #[test]
    fn browse_context_detects_result_pending_state() {
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

    #[test]
    fn browse_result_cell_escape_with_draft_discards_edit() {
        let action = resolve(
            VimCommand::ModeTransition(VimModeTransition::Escape),
            VimSurfaceContext::Browse(BrowseVimContext::Result(ResultVimContext {
                mode: ResultNavMode::CellActive,
                has_pending_draft: true,
                yank_pending: false,
                delete_pending: false,
            })),
        );

        assert!(matches!(action, Some(Action::ResultDiscardCellEdit)));
    }

    #[test]
    fn browse_result_navigation_matrix_still_matches_existing_actions() {
        let action = resolve(
            VimCommand::Navigation(VimNavigation::MoveDown),
            VimSurfaceContext::Browse(BrowseVimContext::Result(ResultVimContext {
                mode: ResultNavMode::Scroll,
                has_pending_draft: false,
                yank_pending: false,
                delete_pending: false,
            })),
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
    fn browse_explorer_scroll_cursor_center_resolves_to_scroll_to_cursor() {
        let action = resolve(
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
    fn browse_inspector_viewport_navigation_resolves_to_none_action() {
        let action = resolve(
            VimCommand::Navigation(VimNavigation::ViewportTop),
            VimSurfaceContext::Browse(BrowseVimContext::Inspector(InspectorVimContext::Other)),
        );

        assert!(matches!(action, Some(Action::None)));
    }

    #[test]
    fn browse_result_cell_horizontal_navigation_uses_cell_actions() {
        let ctx = VimSurfaceContext::Browse(BrowseVimContext::Result(ResultVimContext {
            mode: ResultNavMode::CellActive,
            has_pending_draft: false,
            yank_pending: false,
            delete_pending: false,
        }));

        assert!(matches!(
            resolve(VimCommand::Navigation(VimNavigation::MoveLeft), ctx),
            Some(Action::ResultCellLeft)
        ));
        assert!(matches!(
            resolve(VimCommand::Navigation(VimNavigation::MoveRight), ctx),
            Some(Action::ResultCellRight)
        ));
    }

    #[test]
    fn browse_operator_matrix_covers_row_cell_and_inspector() {
        let row_ctx = BrowseVimContext::Result(ResultVimContext {
            mode: ResultNavMode::RowActive,
            has_pending_draft: false,
            yank_pending: false,
            delete_pending: false,
        });
        let row_pending_ctx = BrowseVimContext::Result(ResultVimContext {
            yank_pending: true,
            delete_pending: true,
            ..match row_ctx {
                BrowseVimContext::Result(ctx) => ctx,
                _ => unreachable!(),
            }
        });
        let cell_ctx = BrowseVimContext::Result(ResultVimContext {
            mode: ResultNavMode::CellActive,
            has_pending_draft: false,
            yank_pending: false,
            delete_pending: false,
        });

        assert!(matches!(
            resolve(
                VimCommand::Operator(VimOperator::Yank),
                VimSurfaceContext::Browse(row_ctx)
            ),
            Some(Action::ResultRowYankOperatorPending)
        ));
        assert!(matches!(
            resolve(
                VimCommand::Operator(VimOperator::Yank),
                VimSurfaceContext::Browse(row_pending_ctx)
            ),
            Some(Action::ResultRowYank)
        ));
        assert!(matches!(
            resolve(
                VimCommand::Operator(VimOperator::Delete),
                VimSurfaceContext::Browse(row_ctx)
            ),
            Some(Action::ResultDeleteOperatorPending)
        ));
        assert!(matches!(
            resolve(
                VimCommand::Operator(VimOperator::Delete),
                VimSurfaceContext::Browse(row_pending_ctx)
            ),
            Some(Action::StageRowForDelete)
        ));
        assert!(matches!(
            resolve(
                VimCommand::Operator(VimOperator::Yank),
                VimSurfaceContext::Browse(cell_ctx)
            ),
            Some(Action::ResultCellYank)
        ));
        assert!(matches!(
            resolve(
                VimCommand::Operator(VimOperator::Yank),
                VimSurfaceContext::Browse(BrowseVimContext::Inspector(InspectorVimContext::Ddl))
            ),
            Some(Action::DdlYank)
        ));
    }

    #[test]
    fn sql_query_normal_shares_insert_and_yank() {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal);

        assert!(matches!(
            resolve_command(&combo(Key::Char('i')), ctx),
            Some(Action::SqlModalEnterInsert)
        ));
        assert!(matches!(
            resolve_command(&combo(Key::Enter), ctx),
            Some(Action::SqlModalEnterInsert)
        ));
        assert!(matches!(
            resolve_command(&combo(Key::Char('y')), ctx),
            Some(Action::SqlModalYank)
        ));
    }

    #[test]
    fn sql_plan_viewer_shares_ctrl_n_and_ctrl_p_scroll() {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::PlanViewer);

        assert!(matches!(
            resolve_command(&combo_ctrl(Key::Char('n')), ctx),
            Some(Action::Scroll {
                target: ScrollTarget::ExplainPlan,
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Line,
            })
        ));
        assert!(matches!(
            resolve_command(&combo_ctrl(Key::Char('p')), ctx),
            Some(Action::Scroll {
                target: ScrollTarget::ExplainPlan,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::Line,
            })
        ));
    }

    #[test]
    fn jsonb_viewing_shares_mode_navigation_search_and_yank() {
        let ctx = VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Viewing);

        assert!(matches!(
            resolve_command(&combo(Key::Enter), ctx),
            Some(Action::JsonbEnterEdit)
        ));
        assert!(matches!(
            resolve_command(&combo(Key::Char('n')), ctx),
            Some(Action::JsonbSearchNext)
        ));
        assert!(matches!(
            resolve_command(&combo(Key::Char('y')), ctx),
            Some(Action::JsonbYankAll)
        ));
        assert!(matches!(
            resolve_command(&combo(Key::Char('h')), ctx),
            Some(Action::TextMoveCursor {
                target: InputTarget::JsonbEdit,
                direction: CursorMove::Left,
            })
        ));
    }

    #[test]
    fn browse_result_search_continuation_stays_unsupported() {
        let action = resolve(
            VimCommand::SearchContinuation(SearchContinuation::Next),
            VimSurfaceContext::Browse(BrowseVimContext::Result(ResultVimContext {
                mode: ResultNavMode::Scroll,
                has_pending_draft: false,
                yank_pending: false,
                delete_pending: false,
            })),
        );

        assert!(action.is_none());
    }
}
