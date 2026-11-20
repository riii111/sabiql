use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;

pub(in crate::update) fn compare_edit_query(state: &mut AppState) -> Vec<Effect> {
    if let Some(ref right) = state.explain.right {
        let query = right.full_query.clone();
        state.sql_modal.load_query_for_editing(query);
    }
    vec![]
}

pub(in crate::update) fn sql_modal_next_tab(state: &mut AppState) -> Vec<Effect> {
    let tab = state
        .session
        .active_engine_feature_profile()
        .next_sql_modal_tab(state.sql_modal.active_tab());
    state.sql_modal.set_active_tab(tab);
    vec![]
}

pub(in crate::update) fn sql_modal_prev_tab(state: &mut AppState) -> Vec<Effect> {
    let tab = state
        .session
        .active_engine_feature_profile()
        .prev_sql_modal_tab(state.sql_modal.active_tab());
    state.sql_modal.set_active_tab(tab);
    vec![]
}
