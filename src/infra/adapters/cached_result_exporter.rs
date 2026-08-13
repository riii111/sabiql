use async_trait::async_trait;
use sabiql_app::domain::QueryValue;
use sabiql_app::ports::outbound::{CachedResultExporter, DbOperationError};

use crate::adapters::csv_export::{CsvFileWriter, export_to_downloads};

#[cfg(test)]
use crate::adapters::csv_export::CSV_FLUSH_THRESHOLD;

#[derive(Debug, Default, Clone, Copy)]
pub struct CsvCachedResultExporter;

#[async_trait]
impl CachedResultExporter for CsvCachedResultExporter {
    async fn export_cached_result_to_csv(
        &self,
        file_name: String,
        columns: Vec<String>,
        values: Vec<Vec<QueryValue>>,
    ) -> Result<std::path::PathBuf, DbOperationError> {
        export_to_downloads(&file_name, |path| {
            write_cached_result_csv(path, columns, values)
        })
        .await
    }
}

fn cached_csv_cell(value: &QueryValue) -> String {
    match value {
        QueryValue::Null => String::new(),
        QueryValue::Text(text) | QueryValue::SqlLiteral(text) => text.clone(),
        QueryValue::Blob(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02X}");
            }
            hex
        }
    }
}

async fn write_cached_result_csv(
    path: std::path::PathBuf,
    columns: Vec<String>,
    values: Vec<Vec<QueryValue>>,
) -> Result<(), DbOperationError> {
    let mut file = CsvFileWriter::create(path).await?;
    file.write_record(columns.iter()).await?;
    for row in &values {
        file.write_record(row.iter().map(cached_csv_cell)).await?;
    }
    file.finish().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    mod cached_csv_cell_tests {
        use super::*;

        #[test]
        fn null_is_empty_field() {
            assert_eq!(cached_csv_cell(&QueryValue::Null), "");
        }

        #[test]
        fn blob_is_uppercase_hex() {
            assert_eq!(cached_csv_cell(&QueryValue::Blob(vec![0xAB, 0xCD])), "ABCD");
        }

        #[test]
        fn text_preserves_embedded_nul_byte() {
            assert_eq!(cached_csv_cell(&QueryValue::text("a\0bc")), "a\0bc");
        }

        #[test]
        fn text_is_not_display_form() {
            assert_ne!(
                cached_csv_cell(&QueryValue::Null),
                QueryValue::Null.display_value()
            );
        }
    }

    mod export_cached_result_to_csv {
        use super::*;

        #[tokio::test]
        async fn writes_columns_and_rows() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("export.csv");

            write_cached_result_csv(
                path.clone(),
                vec!["id".to_string(), "payload".to_string()],
                vec![vec![
                    QueryValue::SqlLiteral("1".to_string()),
                    QueryValue::Blob(vec![0xAB, 0xCD]),
                ]],
            )
            .await
            .unwrap();

            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                "id,payload\n1,ABCD\n"
            );
        }

        #[tokio::test]
        async fn writes_header_for_empty_cached_result() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("export.csv");

            write_cached_result_csv(
                path.clone(),
                vec!["id".to_string(), "payload".to_string()],
                vec![],
            )
            .await
            .unwrap();

            assert_eq!(std::fs::read_to_string(path).unwrap(), "id,payload\n");
        }

        #[tokio::test]
        async fn writes_embedded_nul_text_without_display_escaping() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("export.csv");

            write_cached_result_csv(
                path.clone(),
                vec!["payload".to_string()],
                vec![vec![QueryValue::text("a\0bc")]],
            )
            .await
            .unwrap();

            assert_eq!(std::fs::read(path).unwrap(), b"payload\na\0bc\n");
        }

        #[tokio::test]
        async fn flushes_incrementally_when_data_exceeds_threshold() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("large_export.csv");
            let big_value = "x".repeat(CSV_FLUSH_THRESHOLD + 1);

            write_cached_result_csv(
                path.clone(),
                vec!["data".to_string()],
                vec![vec![QueryValue::Text(big_value.clone())]],
            )
            .await
            .unwrap();

            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                format!("data\n{big_value}\n")
            );
        }

        #[tokio::test]
        async fn returns_error_when_file_cannot_be_created() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("missing").join("export.csv");

            let error = write_cached_result_csv(path, vec!["id".to_string()], vec![])
                .await
                .unwrap_err();

            assert!(matches!(error, DbOperationError::QueryFailed(_)));
        }
    }
}
