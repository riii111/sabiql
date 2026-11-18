use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::detail_view::DetailSearchState;
use crate::model::shared::flash_timer::FlashId;
use crate::update::action::{Action, CopyTarget, DetailTarget, SearchStep};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::{clipboard_unavailable, find_text_matches};

use super::json::jump_to_current_match;
use crate::model::shared::text_input::TextEdit;

pub(in crate::update) fn reduce_detail_search(
    state: &mut AppState,
    action: &Action,
) -> DispatchResult {
    match action {
        Action::DetailSearch { target, step } => {
            apply_search_step(state, *target, *step);
            DispatchResult::handled()
        }
        Action::DetailYankAll(target) => {
            DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                content: yank_content(state, *target),
                on_success: Box::new(Action::CopySucceeded(CopyTarget::Panel(flash_id(*target)))),
                on_failure: Some(Box::new(clipboard_unavailable())),
            }])
        }
        _ => DispatchResult::pass(),
    }
}

pub(in crate::update) fn refresh_search_matches(state: &mut AppState, target: DetailTarget) {
    let (query, content) = match target {
        DetailTarget::JsonDetail => (
            state.json_detail.search().input().content(),
            state.json_detail.editor().content(),
        ),
        DetailTarget::CellDetail => (
            state.cell_detail.search().input().content(),
            state.cell_detail.content(),
        ),
    };
    let matches = find_text_matches(content, query);
    search_mut(state, target).set_matches(matches);
}

fn apply_search_step(state: &mut AppState, target: DetailTarget, step: SearchStep) {
    match step {
        SearchStep::Enter => match target {
            DetailTarget::JsonDetail => state.json_detail.enter_search(),
            DetailTarget::CellDetail => state.cell_detail.enter_search(),
        },
        SearchStep::Exit => exit_search(state, target),
        SearchStep::Submit => {
            exit_search(state, target);
            reveal_current_match(state, target);
        }
        SearchStep::Next => {
            search_mut(state, target).advance_to_next_match();
            reveal_current_match(state, target);
        }
        SearchStep::Prev => {
            search_mut(state, target).advance_to_prev_match();
            reveal_current_match(state, target);
        }
    }
}

fn exit_search(state: &mut AppState, target: DetailTarget) {
    match target {
        DetailTarget::JsonDetail => state.json_detail.exit_search(),
        DetailTarget::CellDetail => state.cell_detail.exit_search(),
    }
}

fn reveal_current_match(state: &mut AppState, target: DetailTarget) {
    match target {
        DetailTarget::JsonDetail => jump_to_current_match(state),
        DetailTarget::CellDetail => state.cell_detail.scroll_to_match(),
    }
}

fn search_mut(state: &mut AppState, target: DetailTarget) -> &mut DetailSearchState {
    match target {
        DetailTarget::JsonDetail => state.json_detail.search_mut(),
        DetailTarget::CellDetail => state.cell_detail.search_mut(),
    }
}

fn yank_content(state: &AppState, target: DetailTarget) -> String {
    match target {
        DetailTarget::JsonDetail => state.json_detail.current_json_for_yank(),
        DetailTarget::CellDetail => state.cell_detail.content().to_string(),
    }
}

fn flash_id(target: DetailTarget) -> FlashId {
    match target {
        DetailTarget::JsonDetail => FlashId::JsonDetail,
        DetailTarget::CellDetail => FlashId::CellDetail,
    }
}

pub(in crate::update) fn edit_search_query(
    state: &mut AppState,
    target: DetailTarget,
    edit: TextEdit,
    yank_text: Option<&str>,
) -> String {
    let killed = search_mut(state, target)
        .input_mut()
        .apply_edit(edit, yank_text);
    if !matches!(edit, TextEdit::MoveCursor(_)) {
        refresh_search_matches(state, target);
    }
    killed
}
