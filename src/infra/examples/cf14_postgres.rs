use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sabiql_app::ports::outbound::{MetadataProvider, QueryExecutor};
use sabiql_infra::adapters::PostgresAdapter;
use serde_json::json;

const DEFAULT_DSN: &str = "postgres://fixture@127.0.0.1:1/app?sslmode=disable";

fn dsn() -> String {
    std::env::var("CF14_DSN").unwrap_or_else(|_| DEFAULT_DSN.to_string())
}

#[expect(
    clippy::print_stdout,
    reason = "benchmark emits machine-readable samples"
)]
#[tokio::main]
async fn main() {
    let operation = std::env::args().nth(1).expect("operation");
    let dsn = dsn();
    let adapter = PostgresAdapter::new();
    for phase in ["fresh-harness", "repeated-harness"] {
        let start_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        #[expect(clippy::disallowed_methods, reason = "benchmark wall-clock boundary")]
        let start = Instant::now();
        let result = match operation.as_str() {
            "startup" => adapter.fetch_effective_user(&dsn).await.map(|_| ()),
            "db_selection" => {
                async {
                    adapter.fetch_metadata(&dsn).await?;
                    adapter.fetch_effective_user(&dsn).await?;
                    Ok(())
                }
                .await
            }
            "metadata_reload" => adapter.fetch_metadata(&dsn).await.map(|_| ()),
            "table_selection" => {
                let (detail, preview) = tokio::join!(
                    adapter.fetch_table_detail(&dsn, "app", "items"),
                    adapter.execute_preview(&dsn, "app", "items", 50, 0)
                );
                detail.and_then(|_| preview.map(|_| ()))
            }
            "inspector" => adapter
                .fetch_table_detail(&dsn, "app", "items")
                .await
                .map(|_| ()),
            "preview" => adapter
                .execute_preview(&dsn, "app", "items", 50, 0)
                .await
                .map(|_| ()),
            "page" => adapter
                .execute_preview(&dsn, "app", "items", 50, 50)
                .await
                .map(|_| ()),
            "completion" => adapter
                .fetch_table_columns_and_fks(&dsn, "app", "items")
                .await
                .map(|_| ()),
            "er_metadata" => adapter.fetch_table_signatures(&dsn).await.map(|_| ()),
            _ => panic!("unknown operation"),
        };
        println!(
            "{}",
            json!({"phase": phase, "start_ns": start_ns, "end_ns": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(), "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0, "error": result.err().map(|error| format!("{error:?}"))})
        );
    }
}
