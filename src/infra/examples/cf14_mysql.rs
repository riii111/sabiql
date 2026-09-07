use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sabiql_app::ports::outbound::{MetadataProvider, MySqlConnectionProbe, QueryExecutor};
use sabiql_infra::adapters::mysql::MySqlAdapter;
use serde_json::json;

// This executable accepts only the synthetic fixture; PATH is isolated by run.py.
const DSN: &str = "mysql://fixture@127.0.0.1:1/app?ssl-mode=DISABLED";

#[expect(
    clippy::print_stdout,
    reason = "benchmark emits machine-readable samples"
)]
#[tokio::main]
async fn main() {
    let operation = std::env::args().nth(1).expect("operation");
    let adapter = MySqlAdapter::new();
    for phase in ["fresh-harness", "repeated-harness"] {
        let start_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        #[expect(clippy::disallowed_methods, reason = "benchmark wall-clock boundary")]
        let start = Instant::now();
        let result = match operation.as_str() {
            "cancel" => {
                let task = tokio::spawn(async { MySqlAdapter::new().fetch_metadata(DSN).await });
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                task.abort();
                let outcome = task.await;
                assert!(outcome.unwrap_err().is_cancelled());
                Ok(())
            }
            "startup" => adapter.probe(DSN).await.map(|_| ()),
            "db_selection" => {
                async {
                    adapter.probe(DSN).await?;
                    adapter.fetch_metadata(DSN).await?;
                    adapter.fetch_effective_user(DSN).await?;
                    Ok(())
                }
                .await
            }
            "metadata_reload" => adapter.fetch_metadata(DSN).await.map(|_| ()),
            "table_selection" => {
                let (detail, preview) = tokio::join!(
                    adapter.fetch_table_detail(DSN, "app", "items"),
                    adapter.execute_preview(DSN, "app", "items", 50, 0)
                );
                detail.and_then(|_| preview.map(|_| ()))
            }
            "inspector" => adapter
                .fetch_table_detail(DSN, "app", "items")
                .await
                .map(|_| ()),
            "preview" => adapter
                .execute_preview(DSN, "app", "items", 50, 0)
                .await
                .map(|_| ()),
            "page" => adapter
                .execute_preview(DSN, "app", "items", 50, 50)
                .await
                .map(|_| ()),
            "completion" => adapter
                .fetch_table_columns_and_fks(DSN, "app", "items")
                .await
                .map(|_| ()),
            "er_metadata" => adapter.fetch_table_signatures(DSN).await.map(|_| ()),
            _ => panic!("unknown operation"),
        };
        println!(
            "{}",
            json!({"phase": phase, "start_ns": start_ns, "end_ns": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(), "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0, "error": result.err().map(|error| format!("{error:?}"))})
        );
    }
}
