use std::sync::Arc;

use crate::cmd::cache_flow_driver::CacheFlowDriver;
use crate::domain::{
    Column, ColumnAttributes, ConnectionId, DatabaseMetadata, DatabaseType, FkAction, ForeignKey,
    Table, TableSignature, TableSignatureSnapshot, TableSummary,
};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::completion::{CompletionCandidate, CompletionKind};
use crate::test_support::table::minimal as minimal_table;
use crate::update::action::{Action, ConnectionTarget, SmartErRefreshFetched};

const POSTGRES_DSN: &str = "postgres://localhost/app";

fn metadata(names: &[&str]) -> Arc<DatabaseMetadata> {
    let mut metadata = DatabaseMetadata::new("app".to_string());
    metadata.table_summaries = names
        .iter()
        .map(|name| TableSummary::new("public".to_string(), (*name).to_string(), None, false))
        .collect();
    Arc::new(metadata)
}

fn table(name: &str, columns: &[&str], references: &[&str]) -> Table {
    let mut table = minimal_table("public", name);
    table.columns = columns
        .iter()
        .enumerate()
        .map(|(index, column)| Column {
            name: (*column).to_string(),
            data_type: "integer".to_string(),
            attributes: ColumnAttributes::NULLABLE,
            ordinal_position: i32::try_from(index).unwrap() + 1,
            default: None,
            comment: None,
            character_set_name: None,
            collation_name: None,
            generation_expression: None,
            generation_kind: None,
        })
        .collect();
    table.foreign_keys = references
        .iter()
        .map(|target| ForeignKey {
            name: format!("{name}_{target}_fk"),
            from_schema: "public".to_string(),
            from_table: name.to_string(),
            from_columns: vec![format!("{target}_id")],
            to_schema: "public".to_string(),
            to_table: (*target).to_string(),
            to_columns: vec!["id".to_string()],
            on_delete: FkAction::NoAction,
            on_update: FkAction::NoAction,
            reference_resolved: true,
        })
        .collect();
    table
}

fn postgres_state(tables: &[&str]) -> AppState {
    let mut state = AppState::new("test".to_string());
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "postgres",
        DatabaseType::PostgreSQL,
        POSTGRES_DSN,
    );
    state.session.mark_connected(metadata(tables));
    state
}

fn open_sql_modal(state: &mut AppState, content: &str) {
    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content(content.to_string());
}

fn candidate_texts(state: &AppState) -> Vec<String> {
    state
        .sql_modal
        .completion()
        .candidates
        .iter()
        .map(|candidate| candidate.text.clone())
        .collect()
}

fn signatures(names: &[&str]) -> Arc<TableSignatureSnapshot> {
    Arc::new(TableSignatureSnapshot {
        signatures: names
            .iter()
            .map(|name| TableSignature {
                schema: "public".to_string(),
                name: (*name).to_string(),
                signature: "v1".to_string(),
            })
            .collect(),
        prefetched_table_details: Vec::new(),
    })
}

async fn open_er_diagram(driver: &mut CacheFlowDriver, tables: &[&str]) {
    driver.dispatch(Action::ErOpenDiagram).await;
    let (dsn, run_id) = driver
        .smart_refresh_run()
        .expect("opening the ER diagram should start a smart refresh");
    driver
        .dispatch(Action::SmartErRefreshFetched(SmartErRefreshFetched {
            dsn,
            run_id,
            new_metadata: metadata(tables),
            signature_snapshot: signatures(tables),
        }))
        .await;
}

mod completion {
    use super::*;

    #[tokio::test]
    async fn prefetches_referenced_table_then_offers_its_columns() {
        let mut state = postgres_state(&["users", "orders"]);
        open_sql_modal(&mut state, "SELECT * FROM public.users WHERE ");
        let mut driver = CacheFlowDriver::new(state);

        driver.dispatch(Action::CompletionRequest).await;
        let fetched_before_completion = driver.fetched_tables().to_vec();
        driver
            .complete_fetch("public.users", table("users", &["id", "email"], &[]))
            .await;

        assert_eq!(fetched_before_completion, ["public.users"]);
        assert!(driver.is_cached("public.users"));
        assert!(candidate_texts(&driver.state).contains(&"email".to_string()));
    }

    #[tokio::test]
    async fn mysql_offers_active_database() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_target(
            &ConnectionId::new(),
            "mysql",
            DatabaseType::MySQL,
            "mysql://user@localhost:3306/app",
            Some("app"),
        );
        state.session.mark_connected(metadata(&["users"]));
        open_sql_modal(&mut state, "SELECT * FROM ");
        let mut driver = CacheFlowDriver::new(state);

        driver.dispatch(Action::CompletionRequest).await;

        assert!(
            driver
                .state
                .sql_modal
                .completion()
                .candidates
                .contains(&CompletionCandidate {
                    text: "`app`".to_string(),
                    kind: CompletionKind::Database,
                    score: 120,
                })
        );
    }

    #[tokio::test]
    async fn er_prefetch_completion_keeps_candidate_selection() {
        let mut state = postgres_state(&["users", "orders"]);
        open_sql_modal(&mut state, "SELECT * FROM ");
        let mut driver = CacheFlowDriver::new(state);
        driver.dispatch(Action::CompletionRequest).await;
        driver.state.sql_modal.completion_next();
        let candidates_before = candidate_texts(&driver.state);

        driver.dispatch(Action::StartErPrefetchAll).await;
        driver
            .complete_fetch("public.users", table("users", &["id"], &[]))
            .await;

        assert_eq!(driver.state.sql_modal.completion().selected_index, 1);
        assert_eq!(candidate_texts(&driver.state), candidates_before);
    }
}

mod cache_lifetime {
    use super::*;

    async fn driver_with_cached_users() -> CacheFlowDriver {
        let mut state = postgres_state(&["users"]);
        open_sql_modal(&mut state, "SELECT * FROM public.users WHERE ");
        let mut driver = CacheFlowDriver::new(state);
        driver.dispatch(Action::CompletionRequest).await;
        driver
            .complete_fetch("public.users", table("users", &["id", "email"], &[]))
            .await;
        assert!(driver.is_cached("public.users"));
        driver
    }

    #[tokio::test]
    async fn metadata_reload_forgets_cached_tables() {
        let mut driver = driver_with_cached_users().await;

        driver.dispatch(Action::ReloadMetadata).await;
        let run_id = driver
            .metadata_fetch_run()
            .expect("reload should fetch metadata");
        driver
            .dispatch(Action::MetadataLoaded {
                run_id,
                metadata: metadata(&["users"]),
            })
            .await;
        driver.state.modal.set_mode(InputMode::SqlModal);
        driver.dispatch(Action::CompletionRequest).await;

        assert!(!driver.is_cached("public.users"));
        assert_eq!(driver.fetched_tables(), ["public.users", "public.users"]);
    }

    #[tokio::test]
    async fn connection_switch_forgets_cached_tables() {
        let mut driver = driver_with_cached_users().await;

        driver
            .dispatch(Action::SwitchConnection(ConnectionTarget {
                id: ConnectionId::new(),
                dsn: "postgres://localhost/other".to_string(),
                name: "other".to_string(),
                database_type: DatabaseType::PostgreSQL,
                database: None,
            }))
            .await;

        assert!(!driver.is_cached("public.users"));
    }

    #[tokio::test]
    async fn stale_prefetch_results_do_not_replace_cached_columns() {
        let mut driver = driver_with_cached_users().await;
        let run_id = driver
            .state
            .table_prefetch
            .active_prefetch_run_id()
            .expect("completion prefetch should stay active");

        for (dsn, stale_run_id) in [
            (POSTGRES_DSN, run_id + 100),
            ("postgres://localhost/other", run_id),
        ] {
            driver
                .dispatch(Action::TableDetailCached {
                    dsn: dsn.to_string(),
                    run_id: stale_run_id,
                    schema: "public".to_string(),
                    table: "users".to_string(),
                    detail: Box::new(table("users", &["legacy_column"], &[])),
                })
                .await;
        }
        driver.dispatch(Action::CompletionRequest).await;

        let candidates = candidate_texts(&driver.state);
        assert!(candidates.contains(&"email".to_string()));
        assert!(!candidates.contains(&"legacy_column".to_string()));
    }

    #[tokio::test]
    async fn pending_mysql_probe_rejects_prefetch_results() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_target(
            &ConnectionId::from_string("mysql-current"),
            "mysql-current",
            DatabaseType::MySQL,
            "mysql://localhost/current",
            Some("current"),
        );
        state.session.mark_connected(metadata(&["users"]));
        let mut driver = CacheFlowDriver::new(state);
        driver
            .dispatch(Action::StartCompletionPrefetch {
                tables: vec!["public.users".to_string()],
            })
            .await;
        let run_id = driver
            .state
            .table_prefetch
            .active_prefetch_run_id()
            .expect("completion prefetch should start");
        let _ = driver.state.session.begin_mysql_connection_probe(
            &ConnectionId::from_string("mysql-target"),
            "mysql-target",
            "mysql://localhost/target",
            Some("target"),
        );

        driver
            .dispatch(Action::TableDetailCached {
                dsn: "mysql://localhost/current".to_string(),
                run_id,
                schema: "public".to_string(),
                table: "users".to_string(),
                detail: Box::new(table("users", &["id"], &[])),
            })
            .await;

        assert!(!driver.is_cached("public.users"));
    }
}

mod er_prefetch {
    use super::*;

    const TABLES: [&str; 5] = ["a", "b", "c", "d", "e"];

    #[tokio::test]
    async fn smart_refresh_fetches_uncached_tables_and_fk_neighbors_once() {
        let mut driver = CacheFlowDriver::new(postgres_state(&TABLES));
        for name in ["b", "c", "e"] {
            driver.seed_cache(table(name, &["id"], &[]));
        }

        open_er_diagram(&mut driver, &TABLES).await;
        let first_wave = driver.pending_fetch_names();
        driver
            .complete_fetch("public.a", table("a", &["id"], &["e"]))
            .await;
        driver
            .complete_fetch("public.d", table("d", &["id"], &["f"]))
            .await;
        let second_wave = driver.pending_fetch_names();
        driver
            .complete_fetch("public.f", table("f", &["id"], &[]))
            .await;

        assert_eq!(first_wave, ["public.a", "public.d"]);
        assert_eq!(second_wave, ["public.f"]);
        assert_eq!(
            driver.fetched_tables(),
            ["public.a", "public.d", "public.f"]
        );
        assert!(driver.max_outstanding_fetches() <= 4);
        assert_eq!(driver.er_generation_count(), 1);
    }

    #[tokio::test]
    async fn queued_cached_tables_complete_without_fetching() {
        let names: Vec<String> = (0..10).map(|index| format!("t{index}")).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut driver = CacheFlowDriver::new(postgres_state(&name_refs));
        driver.dispatch(Action::ErOpenDiagram).await;
        for name in &name_refs[..9] {
            driver.seed_cache(table(name, &["id"], &[]));
        }

        driver.dispatch(Action::StartErPrefetchAll).await;
        let fetched = driver.fetched_tables().to_vec();
        driver
            .complete_fetch("public.t9", table("t9", &["id"], &[]))
            .await;

        assert_eq!(fetched, ["public.t9"]);
        assert!(driver.max_outstanding_fetches() <= 4);
        assert_eq!(driver.er_generation_count(), 1);
    }
}
