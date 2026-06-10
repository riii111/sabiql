mod adapter_postgres;
pub mod harness;

use clap::Parser;

use super::{Args, Command};

#[cfg(not(feature = "self-update"))]
use super::self_update_disabled_message;

#[test]
fn no_subcommand_returns_none() {
    let args = Args::parse_from(["sabiql"]);
    assert!(args.command.is_none());
}

#[test]
fn update_subcommand_is_recognized() {
    let args = Args::parse_from(["sabiql", "update"]);
    assert!(matches!(args.command, Some(Command::Update)));
}

#[test]
#[cfg(not(feature = "self-update"))]
fn disabled_message_contains_version_and_upgrade_guidance() {
    let msg = self_update_disabled_message();
    assert!(msg.contains(env!("CARGO_PKG_VERSION")));
    assert!(msg.contains("brew upgrade sabiql"));
    assert!(msg.contains("cargo install sabiql"));
}

mod dispatch_overflow_fallback {
    use std::time::Instant;

    use sabiql_app::model::app_state::AppState;
    use sabiql_app::update::action::Action;
    use tokio::sync::mpsc;

    use crate::dispatch_overflow_fallback;

    #[test]
    fn requeues_all_actions_and_reports_them_as_deferred() {
        let mut state = AppState::new("test".to_string());
        let (tx, mut rx) = mpsc::channel(8);

        dispatch_overflow_fallback(
            &mut state,
            &tx,
            vec![Action::Render, Action::Render],
            Instant::now(),
        );

        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_ok());
        let error = state.messages.last_error.as_deref().unwrap();
        assert!(error.contains("2 actions deferred"), "got: {error}");
    }

    #[test]
    fn reports_dropped_count_when_channel_is_full() {
        let mut state = AppState::new("test".to_string());
        let (tx, _rx) = mpsc::channel(1);

        dispatch_overflow_fallback(
            &mut state,
            &tx,
            vec![Action::Render, Action::Render, Action::Render],
            Instant::now(),
        );

        let error = state.messages.last_error.as_deref().unwrap();
        assert!(error.contains("2 actions dropped"), "got: {error}");
    }
}
