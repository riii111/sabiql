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
    assert!(args.database.is_none());
    assert!(matches!(args.command, Some(Command::Update)));
}

#[test]
fn database_positional_is_recognized() {
    let args = Args::parse_from(["sabiql", "/tmp/app.db"]);
    assert_eq!(args.database.as_deref(), Some("/tmp/app.db"));
    assert!(args.command.is_none());
}

mod cli_sqlite_startup {
    use std::fs;
    use std::path::Path;

    use sabiql_app::cmd::cli_sqlite::{
        activate_cli_sqlite_connection, connection_id_for_path, resolve_cli_sqlite_target,
    };
    use sabiql_app::model::app_state::AppState;
    use sabiql_app::ports::outbound::{AccessMode, QueryExecutor};
    use sabiql_infra::adapters::{FsSqlitePathValidator, SqliteAdapter};
    use tempfile::tempdir;

    #[test]
    fn resolves_existing_sqlite_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.db");
        fs::write(&path, b"SQLite format 3\0rest").unwrap();

        let target =
            resolve_cli_sqlite_target(path.to_str().unwrap(), &FsSqlitePathValidator).unwrap();

        assert_eq!(target.path(), path.to_str().unwrap());
        assert_eq!(target.dsn(), format!("sqlite://{}", path.display()));
    }

    #[test]
    fn resolves_extensionless_sqlite_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("History");
        fs::write(&path, b"SQLite format 3\0rest").unwrap();

        let target =
            resolve_cli_sqlite_target(path.to_str().unwrap(), &FsSqlitePathValidator).unwrap();

        assert_eq!(target.path(), path.to_str().unwrap());
    }

    #[test]
    fn rejects_non_sqlite_file_before_startup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, b"not a sqlite database").unwrap();

        let error =
            resolve_cli_sqlite_target(path.to_str().unwrap(), &FsSqlitePathValidator).unwrap_err();

        assert!(error.to_string().contains("not a SQLite database"));
    }

    #[test]
    fn rejects_missing_file_before_startup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.db");

        let error =
            resolve_cli_sqlite_target(path.to_str().unwrap(), &FsSqlitePathValidator).unwrap_err();

        assert!(error.to_string().contains("not found"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn activation_pins_canonical_target_for_identity_preview_and_write() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::Builder::new()
            .prefix("sabiql-sab-351-")
            .tempdir_in(".")
            .unwrap();
        let database_a = dir.path().join("app-a.db");
        let database_b = dir.path().join("app-b.db");
        let alias = dir.path().join("current.db");
        fs::write(&database_a, b"").unwrap();
        fs::write(&database_b, b"").unwrap();
        let database_a = fs::canonicalize(database_a).unwrap();
        let database_b = fs::canonicalize(database_b).unwrap();

        let adapter = SqliteAdapter::new();
        for (path, value) in [(&database_a, "A"), (&database_b, "B")] {
            let dsn = format!("sqlite://{}", path.display());
            adapter
                .execute_adhoc(
                    &dsn,
                    "CREATE TABLE items (id INTEGER PRIMARY KEY, value TEXT)",
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
            adapter
                .execute_adhoc(
                    &dsn,
                    &format!("INSERT INTO items VALUES (1, '{value}')"),
                    AccessMode::ReadWrite,
                )
                .await
                .unwrap();
        }

        symlink(&database_a, &alias).unwrap();
        let relative_database_a = database_a
            .strip_prefix(std::env::current_dir().unwrap())
            .unwrap()
            .to_path_buf();

        let activate = |input: &Path| {
            let target =
                resolve_cli_sqlite_target(input.to_str().unwrap(), &FsSqlitePathValidator).unwrap();
            let mut state = AppState::new("test".to_string());
            activate_cli_sqlite_connection(&mut state, &target, &FsSqlitePathValidator).unwrap();
            (
                state.session.active_connection_id().cloned().unwrap(),
                state.session.dsn().unwrap().to_owned(),
                state.session.active_connection_name().unwrap().to_owned(),
                state,
            )
        };

        let absolute = activate(&database_a);
        let relative = activate(&relative_database_a);
        let symlinked = activate(&alias);
        let canonical_a = database_a.to_str().unwrap();
        let expected_dsn = format!("sqlite://{canonical_a}");

        assert_eq!(absolute.0, connection_id_for_path(canonical_a));
        assert_eq!(absolute.0, relative.0);
        assert_eq!(absolute.0, symlinked.0);
        assert_eq!(absolute.1, expected_dsn);
        assert_eq!(relative.1, absolute.1);
        assert_eq!(symlinked.1, absolute.1);
        assert_eq!(symlinked.2, "current.db");

        fs::remove_file(&alias).unwrap();
        symlink(&database_b, &alias).unwrap();

        let preview = adapter
            .execute_preview(&symlinked.1, "main", "items", 10, 0)
            .await
            .unwrap();
        assert_eq!(preview.rows()[0][1], "A");

        let write = adapter
            .execute_write(
                &symlinked.1,
                "UPDATE items SET value = 'A updated' WHERE id = 1",
                AccessMode::ReadWrite,
            )
            .await
            .unwrap();
        assert_eq!(write.affected_rows, 1);

        let updated_a = adapter
            .execute_preview(&symlinked.1, "main", "items", 10, 0)
            .await
            .unwrap();
        assert_eq!(updated_a.rows()[0][1], "A updated");

        let database_b_dsn = format!("sqlite://{}", database_b.display());
        let unchanged_b = adapter
            .execute_preview(&database_b_dsn, "main", "items", 10, 0)
            .await
            .unwrap();
        assert_eq!(unchanged_b.rows()[0][1], "B");

        assert_eq!(symlinked.3.session.dsn(), Some(expected_dsn.as_str()));
    }
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
        let error = state.messages.last_error().unwrap();
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

        let error = state.messages.last_error().unwrap();
        assert!(error.contains("2 actions dropped"), "got: {error}");
    }
}
