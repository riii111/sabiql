//! Pure reducer function for state transitions.
//!
//! The reducer takes the current state and an action, and returns a new state
//! along with a list of effects to be executed. The reducer is pure: it does
//! not perform any I/O, spawn tasks, or acquire the current time.
//!
//! # Purity Rules
//!
//! The reducer MUST NOT:
//! - Call `Instant::now()` or any time-related functions
//! - Perform I/O operations (file, network, etc.)
//! - Spawn async tasks (`tokio::spawn`, etc.)
//! - Access external state (except through the `state` parameter)
//!
//! Time is passed as the `now` parameter to keep the reducer pure and testable.

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::state::AppState;

/// Pure reducer: state transitions only, no I/O.
///
/// # Arguments
///
/// * `state` - Mutable reference to application state
/// * `action` - The action to process
/// * `now` - Current instant (passed in to keep reducer pure)
///
/// # Returns
///
/// Vector of effects to be executed by EffectRunner.
/// An empty vector means no side effects are needed.
#[allow(unused_variables)]
pub fn reduce(state: &mut AppState, action: Action, now: Instant) -> Vec<Effect> {
    match action {
        // Phase 2: Pure UI actions will be migrated here
        // Phase 3: Async actions will be migrated here
        // Phase 4: Special actions (Console, ER) will be migrated here
        _ => vec![], // Placeholder: all actions currently handled in main.rs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> AppState {
        AppState::new("test_project".to_string(), "default".to_string())
    }

    mod skeleton {
        use super::*;

        #[test]
        fn reduce_returns_empty_effects_for_unhandled_action() {
            let mut state = create_test_state();
            let now = Instant::now();

            let effects = reduce(&mut state, Action::None, now);

            assert!(effects.is_empty());
        }
    }
}
