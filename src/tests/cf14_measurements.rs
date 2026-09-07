use ratatui::buffer::Cell;
use std::fmt::Write as _;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sabiql_app::model::app_state::AppState;
use sabiql_app::ports::outbound::{RenderOutput, RenderResult, Renderer};
use sabiql_app::services::AppServices;
use sabiql_infra::adapters::mysql::test_support::export_mysql_csv_to_path_for_test;
use serde_json::{Value, json};

const DSN: &str = "mysql://fixture@127.0.0.1:1/app?ssl-mode=DISABLED";

fn record(value: &Value) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var_os("CF14_SAMPLE").expect("run via scripts/cf14/stage2.py"))
        .unwrap();
    writeln!(file, "{value}").unwrap();
}

fn epoch_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[tokio::test]
#[ignore = "bounded synthetic CSV measurement; requires scripts/cf14/stage2.py"]
async fn csv_export_records_rows_bytes_and_failure_cleanup() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("export.csv");
    let start_ns = epoch_ns();
    let start = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        export_mysql_csv_to_path_for_test(DSN, "SELECT id FROM `app`.`items`", path.clone()),
    )
    .await;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let error = match &result {
        Ok(Ok(_)) => None,
        Ok(Err(error)) => Some(format!("{error:?}")),
        Err(error) => Some(format!("harness deadline: {error}")),
    };
    let csv = fs::read(&path).ok();
    let parts: Vec<_> = fs::read_dir(scratch.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name.to_string_lossy().ends_with(".part"))
        .collect();
    record(
        &json!({"phase":"csv", "start_ns":start_ns, "end_ns":epoch_ns(), "elapsed_ms":elapsed_ms,
        "error":error, "csv_bytes":csv.as_ref().map(Vec::len), "csv_rows":csv.as_ref().map(|bytes| bytes.iter().filter(|&&b| b == b'\n').count().saturating_sub(1)), "part_residual_count":parts.len(), "final_exists":path.exists()}),
    );
    assert!(parts.is_empty());
    assert!(result.is_ok(), "harness deadline is not an export outcome");
    if std::env::var("CF14_CSV_FAULT").as_deref() == Ok("truncate") {
        assert!(error.is_some());
        assert!(!path.exists());
    } else {
        assert!(error.is_none(), "{error:?}");
        let rows: usize = std::env::var("CF14_CSV_ROWS").unwrap().parse().unwrap();
        let mut expected = String::from("id\n");
        for row in 0..rows {
            writeln!(&mut expected, "{row}").unwrap();
        }
        assert_eq!(csv.unwrap(), expected.as_bytes());
    }
}

#[tokio::test]
#[ignore = "bounded connection/cache measurement; requires scripts/cf14/stage2.py"]
async fn connection_switch_records_metadata_and_testbackend_draw() {
    use super::harness::{create_test_state, create_test_terminal_sized, render_and_get_buffer_at};
    use sabiql_app::cmd::completion_engine::CompletionEngine;
    use sabiql_app::cmd::effect::Effect;
    use sabiql_app::cmd::runner::{ConnectionDeps, EffectRunner, ErDeps, QueryDeps, UtilityDeps};
    use sabiql_app::update::action::{Action, ConnectionTarget};
    use sabiql_app::update::reducer::reduce;
    use sabiql_domain::{ConnectionId, DatabaseType};
    use sabiql_infra::adapters::mysql::MySqlAdapter;
    use sabiql_infra::adapters::{
        CsvCachedResultExporter, FileConfigWriter, FileQueryHistoryStore, FsErLogWriter,
        FsSqlitePathValidator, NativeFolderOpener, PgServiceFileReader, SqliteAdapter,
        TomlConnectionStore, TomlSettingsStore,
    };
    use sabiql_infra::export::DotExporter;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::sync::Arc;

    let scratch = tempfile::tempdir().unwrap();
    let services = AppServices::stub();
    let adapter = Arc::new(MySqlAdapter::new());
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let settings = TomlSettingsStore::with_config_dir(scratch.path().to_path_buf());
    let runner = EffectRunner::new(
        adapter.clone(),
        ConnectionDeps {
            dsn_builder: services.dsn_builder.clone(),
            mysql_connection_probe: adapter.clone(),
            connection_store: Arc::new(TomlConnectionStore::with_config_dir(
                scratch.path().to_path_buf(),
            )),
            pg_service_entry_reader: Arc::new(PgServiceFileReader::new()),
            sqlite_path_validator: Arc::new(FsSqlitePathValidator),
        },
        QueryDeps {
            query_executor: adapter,
            query_history_store: Arc::new(FileQueryHistoryStore::new()),
            sqlite_diagnostics: Arc::new(SqliteAdapter::new()),
            cached_result_exporter: Arc::new(CsvCachedResultExporter),
        },
        ErDeps {
            er_exporter: Arc::new(DotExporter::new()),
            config_writer: Arc::new(FileConfigWriter::new()),
            er_log_writer: Arc::new(FsErLogWriter),
        },
        UtilityDeps {
            clipboard: settings.load_clipboard().unwrap(),
            folder_opener: Arc::new(NativeFolderOpener),
        },
        Arc::new(settings),
        tx,
    );
    let mut state = create_test_state();
    let mut terminal = create_test_terminal_sized(120, 40);
    let engine = RefCell::new(CompletionEngine::new());
    let mut unused_renderer = UnexpectedRender;
    for (phase, id, dsn) in [
        ("first-connection", "a", DSN),
        (
            "other-connection",
            "b",
            "mysql://fixture@127.0.0.1:2/app?ssl-mode=DISABLED",
        ),
        ("cached-return", "a", DSN),
    ] {
        let target = ConnectionTarget {
            id: ConnectionId::from_string(id),
            dsn: dsn.to_string(),
            name: format!("fixture-{id}"),
            database_type: DatabaseType::MySQL,
            database: Some("app".to_string()),
        };
        let cache_present = state.connection_caches.contains_key(&target.id);
        assert_eq!(cache_present, phase == "cached-return");
        let start_ns = epoch_ns();
        let start = Instant::now();
        let mut actions = VecDeque::from([Action::SwitchConnection(target.clone())]);
        let mut metadata_ms = None;
        let mut list_state_ms = None;
        let mut first_draw_ms = None;
        let mut refreshed_draw_ms = None;
        let mut trace = Vec::new();
        loop {
            let action = if let Some(action) = actions.pop_front() {
                action
            } else {
                tokio::time::timeout(Duration::from_secs(15), rx.recv())
                    .await
                    .expect("action deadline")
                    .expect("action channel")
            };
            let metadata_loaded = matches!(&action, Action::MetadataLoaded { .. });
            let user_loaded = matches!(&action, Action::EffectiveUserLoaded { .. });
            trace.push(match &action {
                Action::SwitchConnection(_) => "SwitchConnection",
                Action::MySqlConnectionProbeCompleted { .. } => "MySqlConnectionProbeCompleted",
                Action::MetadataLoaded { .. } => "MetadataLoaded",
                Action::EffectiveUserLoaded { .. } => "EffectiveUserLoaded",
                _ => panic!("unexpected action: {action:?}"),
            });
            let effects = reduce(&mut state, action, Instant::now(), &services);
            assert!(
                state.messages.last_error().is_none(),
                "{:?}",
                state.messages.last_error()
            );
            assert!(
                effects.iter().all(|effect| matches!(
                    effect,
                    Effect::CancelTrackedTasks
                        | Effect::CancelMetadataTasks
                        | Effect::ClearCompletionEngineCache
                        | Effect::ProbeMySqlConnection { .. }
                        | Effect::FetchMetadata { .. }
                        | Effect::FetchEffectiveUser { .. }
                        | Effect::DispatchActions(_)
                )),
                "unexpected effects: {effects:?}"
            );
            if metadata_loaded {
                metadata_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
            }
            let list_ready = state.session.active_connection_id() == Some(&target.id)
                && state.session.metadata().is_some();
            if list_ready && list_state_ms.is_none() {
                list_state_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
            }
            let buffer = render_and_get_buffer_at(&mut terminal, &mut state, Instant::now());
            let drawn_ms = start.elapsed().as_secs_f64() * 1000.0;
            if list_ready {
                let text: String = buffer.content.iter().map(Cell::symbol).collect();
                assert!(text.contains("items"), "list was not drawn");
                if first_draw_ms.is_none() {
                    first_draw_ms = Some(drawn_ms);
                }
                if metadata_loaded {
                    refreshed_draw_ms = Some(drawn_ms);
                }
            }
            actions.extend(
                runner
                    .execute_effects(
                        effects,
                        &mut unused_renderer,
                        &mut state,
                        &engine,
                        &services,
                    )
                    .await
                    .unwrap(),
            );
            if user_loaded {
                assert_eq!(state.session.effective_user(), Some("fixture@localhost"));
                break;
            }
            assert!(trace.len() < 20, "bounded action loop");
        }
        record(
            &json!({"phase":phase, "start_ns":start_ns, "end_ns":epoch_ns(), "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
            "cache_present":cache_present, "list_state_ms":list_state_ms, "metadata_loaded_ms":metadata_ms, "first_list_draw_ms":first_draw_ms, "refreshed_list_draw_ms":refreshed_draw_ms, "actions":trace}),
        );
        assert!(metadata_ms.is_some() && first_draw_ms.is_some() && refreshed_draw_ms.is_some());
    }
    runner
        .execute_effects(
            vec![Effect::CancelTrackedTasks],
            &mut unused_renderer,
            &mut state,
            &engine,
            &services,
        )
        .await
        .unwrap();
}

struct UnexpectedRender;

impl Renderer for UnexpectedRender {
    fn draw(&mut self, _: &AppState, _: &AppServices, _: Instant) -> RenderResult<RenderOutput> {
        panic!("this measurement draws via the shared TestBackend harness");
    }
}
