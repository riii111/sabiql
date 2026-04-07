use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
use crate::app::update::action::{Action, CursorMove, InputTarget};
use crate::app::update::input::keybindings::{Key, KeyCombo};
use crate::app::update::input::nav_intent::{
    NavIntent, NavigationContext, map_nav_intent, resolve as resolve_nav_intent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimCommand {
    Navigation(NavIntent),
    ModeTransition(VimModeTransition),
    SearchContinuation(SearchContinuation),
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
pub enum VimSurfaceContext {
    Browse(BrowseVimContext),
    SqlModal(SqlModalVimContext),
    JsonbDetail(JsonbDetailVimContext),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseVimContext {
    Explorer,
    Inspector,
    ResultScroll,
    ResultRowActive,
    ResultCellActive,
    ResultCellActiveWithDraft,
}

impl BrowseVimContext {
    pub fn from_state(state: &AppState) -> Self {
        let result_nav = state.ui.is_focus_mode() || state.ui.focused_pane == FocusedPane::Result;

        if result_nav {
            return match state.result_interaction.selection().mode() {
                ResultNavMode::Scroll => Self::ResultScroll,
                ResultNavMode::RowActive => Self::ResultRowActive,
                ResultNavMode::CellActive => {
                    if state.result_interaction.cell_edit().has_pending_draft() {
                        Self::ResultCellActiveWithDraft
                    } else {
                        Self::ResultCellActive
                    }
                }
            };
        }

        if state.ui.focused_pane == FocusedPane::Inspector {
            Self::Inspector
        } else {
            Self::Explorer
        }
    }

    pub fn is_result(self) -> bool {
        matches!(
            self,
            Self::ResultScroll
                | Self::ResultRowActive
                | Self::ResultCellActive
                | Self::ResultCellActiveWithDraft
        )
    }

    pub fn is_inspector(self) -> bool {
        self == Self::Inspector
    }

    fn navigation_context(self) -> NavigationContext {
        match self {
            Self::Explorer => NavigationContext::Explorer,
            Self::Inspector => NavigationContext::Inspector,
            Self::ResultScroll => NavigationContext::ResultScroll,
            Self::ResultRowActive => NavigationContext::ResultRowActive,
            Self::ResultCellActive | Self::ResultCellActiveWithDraft => {
                NavigationContext::ResultCellActive
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlModalVimContext {
    QueryNormal,
    QueryEditing,
    PlanViewer,
    CompareViewer,
}

impl SqlModalVimContext {
    pub fn from_status(status: &SqlModalStatus, active_tab: SqlModalTab) -> Option<Self> {
        match (status, active_tab) {
            (SqlModalStatus::Editing, SqlModalTab::Sql) => Some(Self::QueryEditing),
            (
                SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error,
                SqlModalTab::Sql,
            ) => Some(Self::QueryNormal),
            (
                SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error,
                SqlModalTab::Plan,
            ) => Some(Self::PlanViewer),
            (
                SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error,
                SqlModalTab::Compare,
            ) => Some(Self::CompareViewer),
            _ => None,
        }
    }
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

    if let Some(intent) = map_nav_intent(combo) {
        return Some(VimCommand::Navigation(intent));
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
        _ => None,
    }
}

pub fn resolve_command(combo: &KeyCombo, ctx: VimSurfaceContext) -> Option<Action> {
    let command = classify_command(combo)?;
    resolve(command, ctx)
}

pub fn resolve(command: VimCommand, ctx: VimSurfaceContext) -> Option<Action> {
    match ctx {
        VimSurfaceContext::Browse(ctx) => resolve_browse(command, ctx),
        VimSurfaceContext::SqlModal(ctx) => resolve_sql_modal(command, ctx),
        VimSurfaceContext::JsonbDetail(ctx) => resolve_jsonb_detail(command, ctx),
    }
}

fn resolve_browse(command: VimCommand, ctx: BrowseVimContext) -> Option<Action> {
    match command {
        VimCommand::Navigation(intent) => {
            Some(resolve_nav_intent(intent, ctx.navigation_context()))
        }
        VimCommand::ModeTransition(VimModeTransition::Escape) => Some(match ctx {
            BrowseVimContext::Explorer | BrowseVimContext::Inspector => Action::Escape,
            BrowseVimContext::ResultScroll => Action::Escape,
            BrowseVimContext::ResultRowActive => Action::ResultExitToScroll,
            BrowseVimContext::ResultCellActive => Action::ResultExitToRowActive,
            BrowseVimContext::ResultCellActiveWithDraft => Action::ResultDiscardCellEdit,
        }),
        VimCommand::ModeTransition(VimModeTransition::ConfirmOrEnter) => Some(match ctx {
            BrowseVimContext::Explorer => Action::ConfirmSelection,
            BrowseVimContext::Inspector => Action::None,
            BrowseVimContext::ResultScroll => Action::ResultEnterRowActive,
            BrowseVimContext::ResultRowActive => Action::ResultEnterCellActive,
            BrowseVimContext::ResultCellActive | BrowseVimContext::ResultCellActiveWithDraft => {
                Action::None
            }
        }),
        VimCommand::ModeTransition(VimModeTransition::Insert) => Some(match ctx {
            BrowseVimContext::ResultCellActive | BrowseVimContext::ResultCellActiveWithDraft => {
                Action::ResultEnterCellEdit
            }
            _ => Action::None,
        }),
        VimCommand::SearchContinuation(_) => None,
    }
}

fn resolve_sql_modal(command: VimCommand, ctx: SqlModalVimContext) -> Option<Action> {
    match ctx {
        SqlModalVimContext::QueryNormal => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseSqlModal),
            VimCommand::ModeTransition(VimModeTransition::Insert)
            | VimCommand::ModeTransition(VimModeTransition::ConfirmOrEnter) => {
                Some(Action::SqlModalEnterInsert)
            }
            _ => None,
        },
        SqlModalVimContext::QueryEditing => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => {
                Some(Action::SqlModalEnterNormal)
            }
            _ => None,
        },
        SqlModalVimContext::PlanViewer | SqlModalVimContext::CompareViewer => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseSqlModal),
            _ => None,
        },
    }
}

fn resolve_jsonb_detail(command: VimCommand, ctx: JsonbDetailVimContext) -> Option<Action> {
    match ctx {
        JsonbDetailVimContext::Viewing => match command {
            VimCommand::Navigation(intent) => jsonb_navigation_action(intent),
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::CloseJsonbDetail),
            VimCommand::ModeTransition(VimModeTransition::Insert)
            | VimCommand::ModeTransition(VimModeTransition::ConfirmOrEnter) => {
                Some(Action::JsonbEnterEdit)
            }
            VimCommand::SearchContinuation(SearchContinuation::Next) => {
                Some(Action::JsonbSearchNext)
            }
            VimCommand::SearchContinuation(SearchContinuation::Prev) => {
                Some(Action::JsonbSearchPrev)
            }
        },
        JsonbDetailVimContext::Editing => match command {
            VimCommand::ModeTransition(VimModeTransition::Escape) => Some(Action::JsonbExitEdit),
            _ => None,
        },
        JsonbDetailVimContext::Searching => None,
    }
}

fn jsonb_navigation_action(intent: NavIntent) -> Option<Action> {
    let direction = match intent {
        NavIntent::MoveLeft => CursorMove::Left,
        NavIntent::MoveRight => CursorMove::Right,
        NavIntent::MoveUp => CursorMove::Up,
        NavIntent::MoveDown => CursorMove::Down,
        NavIntent::MoveToFirst => CursorMove::Home,
        NavIntent::MoveToLast => CursorMove::End,
        _ => return None,
    };

    Some(Action::TextMoveCursor {
        target: InputTarget::JsonbEdit,
        direction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::focused_pane::FocusedPane;
    use crate::app::update::action::{ScrollAmount, ScrollDirection, ScrollTarget, SelectMotion};

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
    fn classify_search_continuation_keys() {
        assert_eq!(
            classify_command(&combo(Key::Char('n'))),
            Some(VimCommand::SearchContinuation(SearchContinuation::Next))
        );
        assert_eq!(
            classify_command(&combo(Key::Char('N'))),
            Some(VimCommand::SearchContinuation(SearchContinuation::Prev))
        );
    }

    #[test]
    fn classify_navigation_aliases() {
        assert_eq!(
            classify_command(&combo_ctrl(Key::Char('n'))),
            Some(VimCommand::Navigation(NavIntent::MoveDown))
        );
        assert_eq!(
            classify_command(&combo(Key::Char('h'))),
            Some(VimCommand::Navigation(NavIntent::MoveLeft))
        );
    }

    #[test]
    fn classify_unsupported_key_returns_none() {
        assert_eq!(classify_command(&combo(Key::Char('y'))), None);
    }

    #[test]
    fn browse_result_cell_escape_with_draft_discards_edit() {
        let action = resolve_command(
            &combo(Key::Esc),
            VimSurfaceContext::Browse(BrowseVimContext::ResultCellActiveWithDraft),
        );

        assert!(matches!(action, Some(Action::ResultDiscardCellEdit)));
    }

    #[test]
    fn browse_result_row_enter_enters_cell_active() {
        let action = resolve_command(
            &combo(Key::Enter),
            VimSurfaceContext::Browse(BrowseVimContext::ResultRowActive),
        );

        assert!(matches!(action, Some(Action::ResultEnterCellActive)));
    }

    #[test]
    fn browse_navigation_still_uses_nav_intent_resolution() {
        let action = resolve_command(
            &combo(Key::Char('j')),
            VimSurfaceContext::Browse(BrowseVimContext::Explorer),
        );

        assert!(matches!(action, Some(Action::Select(SelectMotion::Next))));
    }

    #[test]
    fn browse_result_search_continuation_is_reserved_for_future_wave() {
        let action = resolve_command(
            &combo(Key::Char('n')),
            VimSurfaceContext::Browse(BrowseVimContext::ResultScroll),
        );

        assert!(action.is_none());
    }

    #[test]
    fn browse_context_detects_result_draft_state() {
        let mut state = AppState::new("test".to_string());
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.enter_row(0);
        state.result_interaction.enter_cell(0);
        state
            .result_interaction
            .begin_cell_edit(0, 0, "before".to_string());
        state
            .result_interaction
            .cell_edit_input_mut()
            .set_content("after".to_string());

        assert_eq!(
            BrowseVimContext::from_state(&state),
            BrowseVimContext::ResultCellActiveWithDraft
        );
    }

    #[test]
    fn sql_query_normal_shares_insert_entry_for_i_and_enter() {
        let ctx = VimSurfaceContext::SqlModal(SqlModalVimContext::QueryNormal);

        assert!(matches!(
            resolve_command(&combo(Key::Char('i')), ctx),
            Some(Action::SqlModalEnterInsert)
        ));
        assert!(matches!(
            resolve_command(&combo(Key::Enter), ctx),
            Some(Action::SqlModalEnterInsert)
        ));
    }

    #[test]
    fn sql_editing_escape_returns_to_normal() {
        let action = resolve_command(
            &combo(Key::Esc),
            VimSurfaceContext::SqlModal(SqlModalVimContext::QueryEditing),
        );

        assert!(matches!(action, Some(Action::SqlModalEnterNormal)));
    }

    #[test]
    fn sql_plan_viewer_keeps_enter_unhandled() {
        let action = resolve_command(
            &combo(Key::Enter),
            VimSurfaceContext::SqlModal(SqlModalVimContext::PlanViewer),
        );

        assert!(action.is_none());
    }

    #[test]
    fn sql_context_detects_tab_and_status() {
        assert_eq!(
            SqlModalVimContext::from_status(&SqlModalStatus::Normal, SqlModalTab::Sql),
            Some(SqlModalVimContext::QueryNormal)
        );
        assert_eq!(
            SqlModalVimContext::from_status(&SqlModalStatus::Editing, SqlModalTab::Sql),
            Some(SqlModalVimContext::QueryEditing)
        );
        assert_eq!(
            SqlModalVimContext::from_status(&SqlModalStatus::Normal, SqlModalTab::Plan),
            Some(SqlModalVimContext::PlanViewer)
        );
    }

    #[test]
    fn jsonb_viewing_shares_mode_navigation_and_search() {
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
            resolve_command(&combo(Key::Char('h')), ctx),
            Some(Action::TextMoveCursor {
                target: InputTarget::JsonbEdit,
                direction: CursorMove::Left,
            })
        ));
    }

    #[test]
    fn jsonb_editing_escape_exits_edit_mode() {
        let action = resolve_command(
            &combo(Key::Esc),
            VimSurfaceContext::JsonbDetail(JsonbDetailVimContext::Editing),
        );

        assert!(matches!(action, Some(Action::JsonbExitEdit)));
    }

    #[test]
    fn browse_result_navigation_matrix_still_matches_existing_actions() {
        let action = resolve(
            VimCommand::Navigation(NavIntent::MoveDown),
            VimSurfaceContext::Browse(BrowseVimContext::ResultScroll),
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
}
