use crate::model::app_state::AppState;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::model::shared::ui_state::ResultNavMode;

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
    MoveLineStart,
    MoveLineEnd,
    MoveWordForward,
    MoveWordBackward,
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
    Append,
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
    pub fn is_result(self) -> bool {
        matches!(self, Self::Result(_))
    }

    pub fn is_inspector(self) -> bool {
        matches!(self, Self::Inspector(_))
    }
}

impl From<&AppState> for BrowseVimContext {
    fn from(state: &AppState) -> Self {
        let result_nav = state.ui.is_focus_mode() || state.ui.focused_pane() == FocusedPane::Result;

        if result_nav {
            return Self::Result(ResultVimContext {
                mode: state.result_interaction.selection().mode(),
                has_pending_draft: state.result_interaction.cell_edit().has_pending_draft(),
                yank_pending: state.result_interaction.is_yank_operator_pending(),
                delete_pending: state.result_interaction.is_delete_operator_pending(),
            });
        }

        if state.ui.focused_pane() == FocusedPane::Inspector {
            let inspector_ctx = if state.ui.inspector_tab() == InspectorTab::Ddl {
                InspectorVimContext::Ddl
            } else {
                InspectorVimContext::Other
            };
            Self::Inspector(inspector_ctx)
        } else {
            Self::Explorer
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonbDetailVimContext {
    Viewing,
    Editing,
    Searching,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::app_state::AppState;

    mod browse_context {
        use super::*;

        fn result_context(state: &AppState) -> ResultVimContext {
            let BrowseVimContext::Result(result_ctx) = BrowseVimContext::from(state) else {
                panic!("expected result context");
            };
            result_ctx
        }

        fn result_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.set_focused_pane(FocusedPane::Result);
            state.result_interaction.activate_cell(0, 0);
            state
        }

        #[test]
        fn result_yank_pending_is_detected() {
            let mut state = result_state();
            state.result_interaction.start_yank_operator();

            let result_ctx = result_context(&state);

            assert_eq!(result_ctx.mode, ResultNavMode::CellActive);
            assert!(result_ctx.yank_pending);
            assert!(!result_ctx.delete_pending);
        }

        #[test]
        fn result_delete_pending_is_detected() {
            let mut state = result_state();
            state.result_interaction.start_delete_operator();

            let result_ctx = result_context(&state);

            assert_eq!(result_ctx.mode, ResultNavMode::CellActive);
            assert!(!result_ctx.yank_pending);
            assert!(result_ctx.delete_pending);
        }
    }
}
