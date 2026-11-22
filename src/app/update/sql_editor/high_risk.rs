use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::key_sequence::KeySequenceState;
use crate::model::shared::picker::sanitize_filter_text;
use crate::model::shared::text_input::TextInputState;
use crate::model::sql_editor::modal::{
    HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalContext, SqlModalStatus,
};
use crate::update::action::InputTarget;
use crate::update::helpers::reject_pending_mysql_connection_probe;

use super::helpers::start_adhoc_if_connected;
use crate::model::shared::text_input::TextEdit;

fn high_risk_input_mut(
    sql_modal: &mut SqlModalContext,
    target: InputTarget,
) -> Option<&mut TextInputState> {
    match target {
        InputTarget::SqlModalHighRisk => sql_modal.confirming_high_input_mut(),
        InputTarget::SqlModalAnalyzeHighRisk => sql_modal.confirming_analyze_high_input_mut(),
        _ => None,
    }
}

pub(in crate::update) fn sql_modal_cancel_confirm(state: &mut AppState) -> Vec<Effect> {
    if matches!(
        state.sql_modal.status(),
        SqlModalStatus::ConfirmingHigh { .. } | SqlModalStatus::ConfirmingRisk { .. }
    ) {
        state.sql_modal.cancel_confirmation();
        state.ui.set_key_sequence(KeySequenceState::Idle);
    }
    vec![]
}

pub(in crate::update) fn sql_modal_confirm_execute(
    state: &mut AppState,
    now: Instant,
) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        state.sql_modal.cancel_confirmation();
        return vec![];
    }
    // `matches!` + flag instead of `if let` because the immutable borrow
    // from pattern matching must end before we can mutate `state.sql_modal.status`.
    let matched = matches!(
        state.sql_modal.status(),
        SqlModalStatus::ConfirmingHigh {
            target_name,
            input,
            ..
        } if input.content() == target_name.as_str()
    ) || matches!(
        state.sql_modal.status(),
        SqlModalStatus::ConfirmingRisk { .. }
    );
    if matched {
        let query = state.sql_modal.editor.content().trim().to_string();
        return start_adhoc_if_connected(state, query, now);
    }
    vec![]
}

pub(in crate::update) fn paste_text(
    state: &mut AppState,
    target: InputTarget,
    text: String,
) -> Vec<Effect> {
    if let Some(input) = high_risk_input_mut(&mut state.sql_modal, target) {
        input.insert_str(&sanitize_filter_text(&text));
        input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
    }
    vec![]
}

pub(in crate::update) fn edit_text(
    state: &mut AppState,
    target: InputTarget,
    edit: TextEdit,
    yank_text: Option<&str>,
) -> String {
    high_risk_input_mut(&mut state.sql_modal, target).map_or_default(|input| {
        let killed = input.apply_edit(edit, yank_text);
        input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
        killed
    })
}
