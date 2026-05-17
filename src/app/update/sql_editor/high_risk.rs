use std::time::Instant;

use crate::model::app_state::AppState;
use crate::model::shared::key_sequence::KeySequenceState;
use crate::model::shared::text_input::{TextInputLike, TextInputState};
use crate::model::sql_editor::modal::{
    HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalContext, SqlModalStatus,
};
use crate::update::action::{Action, InputTarget};
use crate::update::dispatch_result::DispatchResult;

use super::helpers::start_adhoc_if_connected;

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

pub(super) fn reduce_high_risk_confirmation(
    state: &mut AppState,
    action: &Action,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::SqlModalCancelConfirm => {
            if matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh { .. }
            ) {
                state.sql_modal.cancel_confirmation();
                state.ui.key_sequence = KeySequenceState::Idle;
                DispatchResult::handled()
            } else {
                DispatchResult::pass()
            }
        }

        // HIGH risk confirmation input (adhoc + EXPLAIN ANALYZE)
        Action::TextInput {
            target: target @ (InputTarget::SqlModalHighRisk | InputTarget::SqlModalAnalyzeHighRisk),
            ch: c,
        } => {
            if let Some(input) = high_risk_input_mut(&mut state.sql_modal, *target) {
                input.insert_char(*c);
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: target @ (InputTarget::SqlModalHighRisk | InputTarget::SqlModalAnalyzeHighRisk),
        } => {
            if let Some(input) = high_risk_input_mut(&mut state.sql_modal, *target) {
                input.backspace();
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            DispatchResult::handled()
        }
        Action::TextMoveCursor {
            target: target @ (InputTarget::SqlModalHighRisk | InputTarget::SqlModalAnalyzeHighRisk),
            direction: movement,
        } => {
            if let Some(input) = high_risk_input_mut(&mut state.sql_modal, *target) {
                input.move_cursor(*movement);
                input.update_viewport(HIGH_RISK_INPUT_VISIBLE_WIDTH);
            }
            DispatchResult::handled()
        }

        Action::SqlModalHighRiskConfirmExecute => {
            // `matches!` + flag instead of `if let` because the immutable borrow
            // from pattern matching must end before we can mutate `state.sql_modal.status`.
            let matched = matches!(
                state.sql_modal.status(),
                SqlModalStatus::ConfirmingHigh {
                    target_name,
                    input,
                    ..
                } if target_name.as_ref().is_some_and(|n| input.content() == n)
            );
            if matched {
                let query = state.sql_modal.editor.content().trim().to_string();
                return start_adhoc_if_connected(state, query, now);
            }
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
