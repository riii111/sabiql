use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::connection::ConnectionEndpoint;
use crate::domain::{
    Column, ColumnAttributes, ConnectionId, DatabaseMetadata, FkAction, ForeignKey, Table,
    TableSignature, TableSignatureSnapshot, TableSummary,
};
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::completion::{CompletionCandidate, CompletionKind};
use crate::test_support::endpoint;
use crate::test_support::table::minimal as minimal_table;
use crate::update::action::{Action, ConnectionTarget, SmartErRefreshFetched};
use crate::update::reducer::reduce;

const MAX_STEPS: usize = 10_000;

struct PendingFetch {
    endpoint: Arc<ConnectionEndpoint>,
    run_id: u64,
    schema: String,
    table: String,
}

struct CacheFlowDriver {
    state: AppState,
    now: Instant,
    queue: VecDeque<Action>,
    channel: VecDeque<Action>,
    pending_fetches: Vec<PendingFetch>,
    fetched_tables: Vec<String>,
    io_effects: Vec<Effect>,
}

impl CacheFlowDriver {
    fn new(state: AppState) -> Self {
        Self {
            state,
            now: Instant::now(),
            queue: VecDeque::new(),
            channel: VecDeque::new(),
            pending_fetches: Vec::new(),
            fetched_tables: Vec::new(),
            io_effects: Vec::new(),
        }
    }

    fn dispatch(&mut self, action: Action) {
        self.queue.push_back(action);
        self.run_until_idle();
    }

    fn complete_fetch(&mut self, qualified_name: &str, table: Table) {
        let index = self
            .pending_fetches
            .iter()
            .position(|fetch| format!("{}.{}", fetch.schema, fetch.table) == qualified_name)
            .unwrap_or_else(|| panic!("no pending fetch for {qualified_name}"));
        let fetch = self.pending_fetches.remove(index);
        self.channel.push_back(Action::TableDetailCached {
            endpoint: fetch.endpoint,
            run_id: fetch.run_id,
            schema: fetch.schema,
            table: fetch.table,
            detail: Box::new(table),
        });
        self.run_until_idle();
    }

    fn seed_cache(&mut self, table: Table) {
        self.state
            .table_detail_cache
            .insert(table.qualified_name(), table);
    }

    fn is_cached(&self, qualified_name: &str) -> bool {
        self.state.table_detail_cache.contains(qualified_name)
    }

    fn pending_fetch_names(&self) -> Vec<String> {
        self.pending_fetches
            .iter()
            .map(|fetch| format!("{}.{}", fetch.schema, fetch.table))
            .collect()
    }

    fn fetched_tables(&self) -> &[String] {
        &self.fetched_tables
    }

    fn er_generation_count(&self) -> usize {
        self.io_effects
            .iter()
            .filter(|effect| matches!(effect, Effect::GenerateErDiagram { .. }))
            .count()
    }

    fn smart_refresh_run(&self) -> Option<(Arc<ConnectionEndpoint>, u64)> {
        self.io_effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::SmartErRefresh { endpoint, run_id } => {
                    Some((Arc::clone(endpoint), *run_id))
                }
                _ => None,
            })
    }

    fn metadata_fetch_run(&self) -> Option<u64> {
        self.io_effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::FetchMetadata { run_id, .. } => Some(*run_id),
                _ => None,
            })
    }

    fn run_until_idle(&mut self) {
        let mut steps = 0;
        loop {
            while let Some(action) = self.queue.pop_front() {
                steps += 1;
                assert!(steps < MAX_STEPS, "action processing did not settle");
                let effects = reduce(&mut self.state, action, self.now);
                for effect in effects {
                    self.handle(effect);
                }
            }
            match self.channel.pop_front() {
                Some(action) => self.queue.push_back(action),
                None => break,
            }
        }
    }

    fn handle(&mut self, effect: Effect) {
        match effect {
            Effect::Render
            | Effect::CancelTrackedTasks
            | Effect::CancelMetadataTasks
            | Effect::CancelConnectionTask
            | Effect::CancelSqliteDiagnostics => {}
            Effect::DispatchActions(actions) => self.queue.extend(actions),
            Effect::SchedulePrefetchQueueProcessing { run_id } => {
                self.channel
                    .push_back(Action::ProcessPrefetchQueue { run_id });
            }
            Effect::PrefetchTableColumnsAndFks {
                endpoint,
                run_id,
                schema,
                table,
            } => {
                self.fetched_tables.push(format!("{schema}.{table}"));
                self.pending_fetches.push(PendingFetch {
                    endpoint,
                    run_id,
                    schema,
                    table,
                });
            }
            other => self.io_effects.push(other),
        }
    }
}

const POSTGRES_DSN: &str = "postgres://localhost/app";

fn metadata(names: &[&str]) -> Arc<DatabaseMetadata> {
    let mut metadata = DatabaseMetadata::new("app".to_string());
    metadata.table_summaries = names
        .iter()
        .map(|name| TableSummary::new("public".to_string(), (*name).to_string(), None))
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
    state.session.activate_connection(
        &ConnectionId::new(),
        "postgres",
        &endpoint::postgres(POSTGRES_DSN),
    );
    state.session.mark_connected(metadata(tables));
    state
}

fn open_sql_modal(state: &mut AppState, content: &str) {
    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.editor.set_content(content.to_string());
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

fn open_er_diagram(driver: &mut CacheFlowDriver, tables: &[&str]) {
    driver.dispatch(Action::ErOpenDiagram);
    let (endpoint, run_id) = driver
        .smart_refresh_run()
        .expect("opening the ER diagram should start a smart refresh");
    driver.dispatch(Action::SmartErRefreshFetched(SmartErRefreshFetched {
        endpoint,
        run_id,
        new_metadata: metadata(tables),
        signature_snapshot: signatures(tables),
    }));
}

mod completion {
    use super::*;

    #[test]
    fn prefetches_referenced_table_then_offers_its_columns() {
        let mut state = postgres_state(&["users", "orders"]);
        open_sql_modal(&mut state, "SELECT * FROM public.users WHERE ");
        let mut driver = CacheFlowDriver::new(state);

        driver.dispatch(Action::CompletionRequest);
        let fetched_before_completion = driver.fetched_tables().to_vec();
        driver.complete_fetch("public.users", table("users", &["id", "email"], &[]));

        assert_eq!(fetched_before_completion, ["public.users"]);
        assert!(driver.is_cached("public.users"));
        assert!(candidate_texts(&driver.state).contains(&"email".to_string()));
    }

    #[test]
    fn mysql_offers_active_database() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection(
            &ConnectionId::new(),
            "mysql",
            &endpoint::mysql("mysql://user@localhost:3306/app", Some("app")),
        );
        state.session.mark_connected(metadata(&["users"]));
        open_sql_modal(&mut state, "SELECT * FROM ");
        let mut driver = CacheFlowDriver::new(state);

        driver.dispatch(Action::CompletionRequest);

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

    #[test]
    fn er_prefetch_completion_keeps_candidate_selection() {
        let mut state = postgres_state(&["users", "orders"]);
        open_sql_modal(&mut state, "SELECT * FROM ");
        let mut driver = CacheFlowDriver::new(state);
        driver.dispatch(Action::CompletionRequest);
        driver.state.sql_modal.completion_next();
        let candidates_before = candidate_texts(&driver.state);

        driver.dispatch(Action::StartErPrefetchAll);
        driver.complete_fetch("public.users", table("users", &["id"], &[]));

        assert_eq!(driver.state.sql_modal.completion().selected_index, 1);
        assert_eq!(candidate_texts(&driver.state), candidates_before);
    }
}

mod cache_lifetime {
    use super::*;

    fn driver_with_cached_users() -> CacheFlowDriver {
        let mut state = postgres_state(&["users"]);
        open_sql_modal(&mut state, "SELECT * FROM public.users WHERE ");
        let mut driver = CacheFlowDriver::new(state);
        driver.dispatch(Action::CompletionRequest);
        driver.complete_fetch("public.users", table("users", &["id", "email"], &[]));
        assert!(driver.is_cached("public.users"));
        driver
    }

    fn assert_completion_columns(driver: &mut CacheFlowDriver, expected: &str, stale: &str) {
        open_sql_modal(&mut driver.state, "SELECT * FROM public.users WHERE ");
        driver.dispatch(Action::CompletionRequest);
        driver.complete_fetch("public.users", table("users", &["id", expected], &[]));
        let candidates = candidate_texts(&driver.state);
        assert!(
            candidates.iter().any(|text| text == expected),
            "{candidates:?}"
        );
        assert!(
            !candidates.iter().any(|text| text == stale),
            "{candidates:?}"
        );
    }

    #[test]
    fn metadata_reload_forgets_cached_tables() {
        let mut driver = driver_with_cached_users();

        driver.dispatch(Action::ReloadMetadata);
        let run_id = driver
            .metadata_fetch_run()
            .expect("reload should fetch metadata");
        driver.dispatch(Action::MetadataLoaded {
            run_id,
            metadata: metadata(&["users"]),
        });
        driver.state.modal.set_mode(InputMode::SqlModal);
        driver.dispatch(Action::CompletionRequest);

        assert!(!driver.is_cached("public.users"));
        assert_eq!(driver.fetched_tables(), ["public.users", "public.users"]);
    }

    #[test]
    fn completion_refetches_same_named_table_when_switching_away_and_back() {
        let mut driver = driver_with_cached_users();
        let original = driver.state.session.active_connection_id().unwrap().clone();

        driver.dispatch(Action::SwitchConnection(ConnectionTarget {
            id: ConnectionId::new(),
            name: "other".to_string(),
            endpoint: endpoint::postgres("postgres://localhost/other"),
        }));

        assert!(!driver.is_cached("public.users"));
        let run_id = driver.metadata_fetch_run().unwrap();
        driver.dispatch(Action::MetadataLoaded {
            run_id,
            metadata: metadata(&["users"]),
        });

        assert_completion_columns(&mut driver, "phone", "email");

        driver.dispatch(Action::SwitchConnection(ConnectionTarget {
            id: original,
            name: "postgres".to_string(),
            endpoint: endpoint::postgres(POSTGRES_DSN),
        }));
        assert_completion_columns(&mut driver, "email", "phone");
    }

    #[test]
    fn stale_prefetch_results_do_not_replace_cached_columns() {
        let mut driver = driver_with_cached_users();
        let run_id = driver
            .state
            .table_prefetch
            .active_prefetch_run_id()
            .expect("completion prefetch should stay active");

        for (dsn, stale_run_id) in [
            (POSTGRES_DSN, run_id + 100),
            ("postgres://localhost/other", run_id),
        ] {
            driver.dispatch(Action::TableDetailCached {
                endpoint: endpoint::postgres(dsn),
                run_id: stale_run_id,
                schema: "public".to_string(),
                table: "users".to_string(),
                detail: Box::new(table("users", &["legacy_column"], &[])),
            });
        }
        driver.dispatch(Action::CompletionRequest);

        let candidates = candidate_texts(&driver.state);
        assert!(candidates.contains(&"email".to_string()));
        assert!(!candidates.contains(&"legacy_column".to_string()));
    }

    #[test]
    fn pending_mysql_probe_rejects_prefetch_results() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection(
            &ConnectionId::from_string("mysql-current"),
            "mysql-current",
            &endpoint::mysql("mysql://localhost/current", Some("current")),
        );
        state.session.mark_connected(metadata(&["users"]));
        let mut driver = CacheFlowDriver::new(state);
        driver.dispatch(Action::StartCompletionPrefetch {
            tables: vec!["public.users".to_string()],
        });
        let run_id = driver
            .state
            .table_prefetch
            .active_prefetch_run_id()
            .expect("completion prefetch should start");
        let _ = driver.state.session.begin_mysql_connection_probe(
            &ConnectionId::from_string("mysql-target"),
            "mysql-target",
            &endpoint::mysql("mysql://localhost/target", Some("target")),
        );

        driver.dispatch(Action::TableDetailCached {
            endpoint: endpoint::mysql("mysql://localhost/current", Some("current")),
            run_id,
            schema: "public".to_string(),
            table: "users".to_string(),
            detail: Box::new(table("users", &["id"], &[])),
        });

        assert!(!driver.is_cached("public.users"));
    }
}

mod er_prefetch {
    use super::*;

    const TABLES: [&str; 5] = ["a", "b", "c", "d", "e"];

    #[test]
    fn smart_refresh_fetches_uncached_tables_and_fk_neighbors_once() {
        let mut driver = CacheFlowDriver::new(postgres_state(&TABLES));
        for name in ["b", "c", "e"] {
            driver.seed_cache(table(name, &["id"], &[]));
        }

        open_er_diagram(&mut driver, &TABLES);
        let first_wave = driver.pending_fetch_names();
        driver.complete_fetch("public.a", table("a", &["id"], &["e"]));
        driver.complete_fetch("public.d", table("d", &["id"], &["f"]));
        let second_wave = driver.pending_fetch_names();
        driver.complete_fetch("public.f", table("f", &["id"], &[]));

        assert_eq!(first_wave, ["public.a", "public.d"]);
        assert_eq!(second_wave, ["public.f"]);
        assert_eq!(
            driver.fetched_tables(),
            ["public.a", "public.d", "public.f"]
        );
        assert_eq!(driver.er_generation_count(), 1);
    }

    #[test]
    fn queued_cached_tables_complete_without_fetching() {
        let names: Vec<String> = (0..10).map(|index| format!("t{index}")).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut driver = CacheFlowDriver::new(postgres_state(&name_refs));
        driver.dispatch(Action::ErOpenDiagram);
        for name in &name_refs[..9] {
            driver.seed_cache(table(name, &["id"], &[]));
        }

        driver.dispatch(Action::StartErPrefetchAll);
        let fetched = driver.fetched_tables().to_vec();
        driver.complete_fetch("public.t9", table("t9", &["id"], &[]));

        assert_eq!(fetched, ["public.t9"]);
        assert_eq!(driver.er_generation_count(), 1);
    }
}
