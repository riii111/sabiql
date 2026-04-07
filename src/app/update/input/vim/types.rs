use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::ui_state::ResultNavMode;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::app_state::AppState;

    #[test]
    fn browse_context_detects_result_pending_state() {
        let mut state = AppState::new("test".to_string());
        state.ui.focused_pane = FocusedPane::Result;
        state.result_interaction.activate_cell(0, 0);
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
