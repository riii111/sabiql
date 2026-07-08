use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sabiql_app::domain::QueryValue;
use sabiql_app::ports::outbound::{CachedResultExporter, DbOperationError};

#[derive(Debug, Default, Clone, Copy)]
pub struct CsvCachedResultExporter;

#[async_trait]
impl CachedResultExporter for CsvCachedResultExporter {
    async fn export_cached_result_to_csv(
        &self,
        path: &Path,
        columns: &[String],
        values: &[Vec<QueryValue>],
    ) -> Result<usize, DbOperationError> {
        let path = path.to_path_buf();
        let columns = columns.to_vec();
        let values = values.to_vec();
        tokio::task::spawn_blocking(move || write_cached_result_csv(path, columns, values))
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?
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

fn write_cached_result_csv(
    path: PathBuf,
    columns: Vec<String>,
    values: Vec<Vec<QueryValue>>,
) -> Result<usize, DbOperationError> {
    let file = std::fs::File::create(path)
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    let mut writer = csv::WriterBuilder::new().from_writer(file);
    writer.write_record(columns)?;
    for row in &values {
        let record: Vec<String> = row.iter().map(cached_csv_cell).collect();
        writer.write_record(&record)?;
    }
    writer
        .flush()
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    Ok(values.len())
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
        async fn writes_columns_rows_and_returns_row_count() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("export.csv");

            let row_count = CsvCachedResultExporter
                .export_cached_result_to_csv(
                    &path,
                    &["id".to_string(), "payload".to_string()],
                    &[vec![
                        QueryValue::SqlLiteral("1".to_string()),
                        QueryValue::Blob(vec![0xAB, 0xCD]),
                    ]],
                )
                .await
                .unwrap();

            assert_eq!(row_count, 1);
            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                "id,payload\n1,ABCD\n"
            );
        }

        #[tokio::test]
        async fn returns_error_when_file_cannot_be_created() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("missing").join("export.csv");

            let error = CsvCachedResultExporter
                .export_cached_result_to_csv(&path, &["id".to_string()], &[])
                .await
                .unwrap_err();

            assert!(matches!(error, DbOperationError::QueryFailed(_)));
        }
    }
}
