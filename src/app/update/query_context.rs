use crate::cmd::effect::Effect;
use crate::model::browse::query_execution::QueryExecution;

/// Adds runtime cancellation after the model has invalidated the query context.
pub fn termination_effects(query: &QueryExecution, follow_up: Vec<Effect>) -> Vec<Effect> {
    debug_assert!(
        !query.is_running(),
        "query context must be invalidated before cancelling its task"
    );

    let mut effects = Vec::with_capacity(follow_up.len() + 1);
    effects.push(Effect::CancelActiveQuery);
    effects.extend(follow_up);
    effects
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn cancellation_precedes_follow_up_effects() {
        let query = QueryExecution::default();

        let effects = termination_effects(&query, vec![Effect::ClearCompletionEngineCache]);

        assert!(matches!(effects[0], Effect::CancelActiveQuery));
        assert!(matches!(effects[1], Effect::ClearCompletionEngineCache));
    }

    #[test]
    #[should_panic(expected = "query context must be invalidated")]
    fn active_query_context_is_rejected() {
        let mut query = QueryExecution::default();
        let _ = query.begin_running(Instant::now());

        let _ = termination_effects(&query, vec![]);
    }
}
