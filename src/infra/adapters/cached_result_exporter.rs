use async_trait::async_trait;
use sabiql_app::domain::QueryValue;
use sabiql_app::ports::outbound::{CachedResultExporter, DbOperationError};
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::adapters::csv_export::export_to_downloads;

const CSV_FLUSH_THRESHOLD: usize = 64 * 1024;

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
    let file = tokio::fs::File::create(path)
        .await
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    let mut file = BufWriter::new(file);
    let mut csv_writer =
        csv::WriterBuilder::new().from_writer(Vec::with_capacity(CSV_FLUSH_THRESHOLD));
    let mut bytes_since_flush = 0;

    csv_writer = write_csv_record(
        csv_writer,
        &mut file,
        columns.iter(),
        &mut bytes_since_flush,
    )
    .await?;
    for row in &values {
        csv_writer = write_csv_record(
            csv_writer,
            &mut file,
            row.iter().map(cached_csv_cell),
            &mut bytes_since_flush,
        )
        .await?;
    }
    let encoded = csv_writer
        .into_inner()
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    file.write_all(&encoded)
        .await
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    file.flush()
        .await
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
    Ok(())
}

async fn write_csv_record<I>(
    mut csv_writer: csv::Writer<Vec<u8>>,
    file: &mut BufWriter<tokio::fs::File>,
    record: I,
    bytes_since_flush: &mut usize,
) -> Result<csv::Writer<Vec<u8>>, DbOperationError>
where
    I: IntoIterator,
    I::Item: AsRef<[u8]>,
{
    csv_writer.write_record(record)?;
    csv_writer
        .flush()
        .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;

    *bytes_since_flush = csv_writer.get_ref().len();
    if *bytes_since_flush >= CSV_FLUSH_THRESHOLD {
        let mut encoded = csv_writer
            .into_inner()
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        file.write_all(&encoded)
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        file.flush()
            .await
            .map_err(|error| DbOperationError::QueryFailed(error.to_string()))?;
        *bytes_since_flush = 0;
        encoded.clear();
        csv_writer = csv::WriterBuilder::new().from_writer(encoded);
    }
    Ok(csv_writer)
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
