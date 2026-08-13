use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::sql_editor::modal::sql_modal_visible_rows;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_completion(
    state: &mut AppState,
    action: &Action,
    _now: Instant,
) -> DispatchResult {
    match action {
        // Completion navigation
        Action::CompletionNext => {
            state.sql_modal.completion_next();
            DispatchResult::handled()
        }
        Action::CompletionPrev => {
            state.sql_modal.completion_prev();
            DispatchResult::handled()
        }
        Action::CompletionDismiss => {
            state.sql_modal.dismiss_completion();
            DispatchResult::handled()
        }
        // Completion accept
        Action::CompletionAccept => {
            state
                .sql_modal
                .accept_selected_completion(sql_modal_visible_rows(state.ui.terminal_height()));
            DispatchResult::handled()
        }

        // Completion trigger/update
        Action::CompletionRequest => DispatchResult::handled_with(vec![Effect::TriggerCompletion]),
        Action::CompletionUpdated {
            candidates,
            trigger_position,
            visible,
            dsn,
            connection_generation,
            database_generation,
            metadata_generation,
        } => {
            if !state.session.is_current_completion_scope(
                dsn.as_deref(),
                *connection_generation,
                *database_generation,
                *metadata_generation,
            ) {
                return DispatchResult::handled();
            }
            state
                .sql_modal
                .apply_completion_update(candidates, *trigger_position, *visible);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::domain::{ConnectionId, DatabaseMetadata, DatabaseType};

    #[test]
    fn stale_completion_update_from_previous_database_is_ignored() {
        let mut state = AppState::new("test".to_string());
        let id = ConnectionId::new();
        state.session.activate_connection_with_target(
            &id,
            "mysql",
            DatabaseType::MySQL,
            "mysql://user@localhost:3306/app",
            Some("app"),
        );
        let metadata_run = state.session.begin_metadata_refresh();
        state
            .session
            .mark_connected(Arc::new(DatabaseMetadata::new("app".to_string())));

        let old_scope = (
            state.session.dsn().map(str::to_string),
            state.session.connection_generation(),
            state.session.database_generation(),
            metadata_run,
        );
        state.session.activate_connection_with_target(
            &id,
            "mysql",
            DatabaseType::MySQL,
            "mysql://user@localhost:3306/analytics",
            Some("analytics"),
        );

        reduce_completion(
            &mut state,
            &Action::CompletionUpdated {
                candidates: vec![],
                trigger_position: 0,
                visible: true,
                dsn: old_scope.0,
                connection_generation: old_scope.1,
                database_generation: old_scope.2,
                metadata_generation: old_scope.3,
            },
            Instant::now(),
        );

        assert!(!state.sql_modal.completion().visible);
    }
}
