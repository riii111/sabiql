use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::MetadataError;
use crate::domain::{QueryResult, QuerySource, WriteExecutionResult};

use super::super::PostgresAdapter;

struct PsqlOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl PostgresAdapter {
    const PGOPTIONS_READ_ONLY: &str = "-c default_transaction_read_only=on";

    async fn run_psql(
        &self,
        dsn: &str,
        extra_args: &[&str],
        query: &str,
        read_only: bool,
    ) -> Result<PsqlOutput, MetadataError> {
        let mut cmd = Command::new("psql");
        if read_only {
            Self::apply_read_only_pgoptions(&mut cmd);
        }
        cmd.arg(dsn).arg("-X").arg("-v").arg("ON_ERROR_STOP=1");

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.arg("-c").arg(query);

        Self::collect_output(&mut cmd, self.timeout_secs).await
    }

    fn apply_read_only_pgoptions(cmd: &mut Command) {
        let merged = match std::env::var("PGOPTIONS") {
            Ok(existing) => format!("{} {}", Self::PGOPTIONS_READ_ONLY, existing),
            Err(_) => Self::PGOPTIONS_READ_ONLY.to_string(),
        };
        cmd.env("PGOPTIONS", merged);
    }

    async fn collect_output(
        cmd: &mut Command,
        timeout_secs: u64,
    ) -> Result<PsqlOutput, MetadataError> {
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| MetadataError::CommandNotFound(e.to_string()))?;

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let result = timeout(Duration::from_secs(timeout_secs), async {
            let (stdout_result, stderr_result) = tokio::join!(
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut out) = stdout_handle {
                        out.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                },
                async {
                    let mut buf = Vec::new();
                    if let Some(ref mut err) = stderr_handle {
                        err.read_to_end(&mut buf).await?;
                    }
                    Ok::<_, std::io::Error>(String::from_utf8_lossy(&buf).into_owned())
                }
            );

            let stdout = stdout_result?;
            let stderr = stderr_result?;
            let status = child.wait().await?;

            Ok::<_, std::io::Error>((status, stdout, stderr))
        })
        .await
        .map_err(|_| MetadataError::Timeout)?
        .map_err(|e| MetadataError::QueryFailed(e.to_string()))?;

        let (status, stdout, stderr) = result;
        Ok(PsqlOutput {
            status,
            stdout,
            stderr,
        })
    }

    pub(in crate::infra::adapters::postgres) async fn execute_query(
        &self,
        dsn: &str,
        query: &str,
    ) -> Result<String, MetadataError> {
        let output = self.run_psql(dsn, &["-t", "-A"], query, false).await?;

        if !output.status.success() {
            return Err(MetadataError::QueryFailed(output.stderr));
        }

        Ok(output.stdout)
    }

    pub(in crate::infra::adapters::postgres) async fn execute_query_raw(
        &self,
        dsn: &str,
        query: &str,
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, MetadataError> {
        let start = Instant::now();

        let output = self.run_psql(dsn, &["--csv"], query, read_only).await?;

        let elapsed = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            return Ok(QueryResult::error(
                query.to_string(),
                output.stderr.trim().to_string(),
                elapsed,
                source,
            ));
        }

        if output.stdout.trim().is_empty() {
            return Ok(QueryResult::success(
                query.to_string(),
                Vec::new(),
                Vec::new(),
                elapsed,
                source,
            ));
        }

        let stdout_trimmed = output.stdout.trim();
        if let Some(tag) = Self::parse_aggregate_command_tag(stdout_trimmed, query) {
            let row_count = tag.affected_rows().unwrap_or(0) as usize;
            let mut result =
                QueryResult::success(query.to_string(), Vec::new(), Vec::new(), elapsed, source);
            result.row_count = row_count;
            return Ok(result.with_command_tag(tag));
        }

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(output.stdout.as_bytes());

        let columns: Vec<String> = reader
            .headers()
            .map_err(|e| MetadataError::QueryFailed(format!("CSV parse error: {}", e)))?
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut rows = Vec::new();
        for result in reader.records() {
            let record = result
                .map_err(|e| MetadataError::QueryFailed(format!("CSV parse error: {}", e)))?;
            let row: Vec<String> = record.iter().map(|s| s.to_string()).collect();
            rows.push(row);
        }

        Ok(QueryResult::success(
            query.to_string(),
            columns,
            rows,
            elapsed,
            source,
        ))
    }

    pub(in crate::infra::adapters::postgres) async fn execute_write_raw(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<WriteExecutionResult, MetadataError> {
        let start = Instant::now();

        let output = self.run_psql(dsn, &[], query, read_only).await?;

        let elapsed = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            return Err(MetadataError::QueryFailed(output.stderr.trim().to_string()));
        }

        let affected_rows = Self::parse_affected_rows(&output.stdout).ok_or_else(|| {
            MetadataError::QueryFailed("Failed to parse affected row count".to_string())
        })?;

        Ok(WriteExecutionResult {
            affected_rows,
            execution_time_ms: elapsed,
        })
    }

    pub(in crate::infra::adapters::postgres) async fn count_rows(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<usize, MetadataError> {
        let output = self.run_psql(dsn, &["-t", "-A"], query, read_only).await?;
        if !output.status.success() {
            return Err(MetadataError::QueryFailed(output.stderr));
        }
        output
            .stdout
            .trim()
            .parse::<usize>()
            .map_err(|e| MetadataError::QueryFailed(format!("Failed to parse COUNT result: {}", e)))
    }

    pub(in crate::infra::adapters::postgres) async fn export_csv_to_file(
        &self,
        dsn: &str,
        query: &str,
        path: &std::path::Path,
        read_only: bool,
    ) -> Result<usize, MetadataError> {
        let mut cmd = Command::new("psql");
        if read_only {
            Self::apply_read_only_pgoptions(&mut cmd);
        }
        cmd.arg(dsn)
            .arg("-X")
            .arg("-v")
            .arg("ON_ERROR_STOP=1")
            .arg("--csv")
            .arg("-c")
            .arg(query);

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| MetadataError::CommandNotFound(e.to_string()))?;

        let stdout = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let file = tokio::fs::File::create(path)
            .await
            .map_err(|e| MetadataError::QueryFailed(format!("Failed to create file: {}", e)))?;
        let mut writer = tokio::io::BufWriter::new(file);

        let result = timeout(Duration::from_secs(self.timeout_secs * 10), async {
            let mut newline_count: usize = 0;
            if let Some(mut out) = stdout {
                let mut buf = [0u8; 8192];
                loop {
                    let n = out.read(&mut buf).await?;
                    if n == 0 {
                        break;
                    }
                    newline_count += buf[..n].iter().filter(|&&b| b == b'\n').count();
                    writer.write_all(&buf[..n]).await?;
                }
                writer.flush().await?;
            }

            let stderr = {
                let mut buf = Vec::new();
                if let Some(ref mut err) = stderr_handle {
                    err.read_to_end(&mut buf).await?;
                }
                String::from_utf8_lossy(&buf).into_owned()
            };

            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, stderr, newline_count))
        })
        .await
        .map_err(|_| MetadataError::Timeout)?
        .map_err(|e| MetadataError::QueryFailed(e.to_string()))?;

        let (status, stderr, newline_count) = result;
        if !status.success() {
            let _ = tokio::fs::remove_file(path).await;
            return Err(MetadataError::QueryFailed(stderr.trim().to_string()));
        }

        // Subtract 1 for the CSV header line
        let row_count = newline_count.saturating_sub(1);
        Ok(row_count)
    }

    pub(in crate::infra::adapters::postgres) async fn fetch_preview_order_columns(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Vec<String>, MetadataError> {
        let query = Self::preview_pk_columns_query(schema, table);
        let raw = self.execute_query(dsn, &query).await?;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(vec![]);
        }

        serde_json::from_str(trimmed).map_err(|e| MetadataError::InvalidJson(e.to_string()))
    }

    pub(in crate::infra::adapters::postgres) fn parse_affected_rows(stdout: &str) -> Option<usize> {
        Self::extract_command_tag(stdout)
            .and_then(|tag| tag.affected_rows())
            .map(|n| n as usize)
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::adapters::postgres::PostgresAdapter;

    mod csv_parsing {
        #[test]
        fn empty_csv_output_has_no_headers() {
            let csv_data = "";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(false)
                .from_reader(csv_data.as_bytes());

            let records: Vec<_> = reader.records().collect();

            assert_eq!(records.len(), 0);
        }

        #[test]
        fn valid_csv_parses_headers_and_rows() {
            let csv_data = "id,name\n1,alice\n2,bob";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv_data.as_bytes());

            let headers: Vec<String> = reader
                .headers()
                .unwrap()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let rows: Vec<_> = reader.records().collect();

            assert_eq!(headers.len(), 2);
            assert_eq!(headers[0], "id");
            assert_eq!(headers[1], "name");
            assert_eq!(rows.len(), 2);
        }

        #[test]
        fn csv_with_multibyte_characters_parses_correctly() {
            let csv_data = "名前,年齢\n太郎,25\n花子,30";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv_data.as_bytes());

            let headers: Vec<String> = reader
                .headers()
                .unwrap()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let first_row = reader.records().next().unwrap().unwrap();

            assert_eq!(headers[0], "名前");
            assert_eq!(first_row.get(0), Some("太郎"));
        }

        #[test]
        fn csv_with_quoted_fields_parses_correctly() {
            let csv_data = "id,description\n1,\"hello, world\"\n2,\"line1\nline2\"";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv_data.as_bytes());

            let rows: Vec<_> = reader.records().map(|r| r.unwrap()).collect();

            assert_eq!(rows[0].get(1), Some("hello, world"));
            assert_eq!(rows[1].get(1), Some("line1\nline2"));
        }

        #[test]
        fn csv_with_empty_values_parses_correctly() {
            let csv_data = "id,name,email\n1,,alice@example.com\n2,bob,";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(csv_data.as_bytes());

            let rows: Vec<_> = reader.records().map(|r| r.unwrap()).collect();

            assert_eq!(rows[0].get(1), Some(""));
            assert_eq!(rows[1].get(2), Some(""));
        }

        #[test]
        fn invalid_csv_returns_error() {
            let csv_data = "id,name\n1,alice\n2,bob,extra";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .flexible(false)
                .from_reader(csv_data.as_bytes());

            reader.headers().unwrap();
            let results: Vec<_> = reader.records().collect();

            assert!(results[1].is_err());
        }

        #[test]
        fn non_csv_output_like_notice_parses_as_header() {
            let non_csv = "NOTICE: some database notice\nNOTICE: another line";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(non_csv.as_bytes());

            let headers = reader.headers();

            assert!(headers.is_ok());
        }

        #[test]
        fn mixed_notice_and_csv_parses_first_line_as_header() {
            let mixed = "id,name\n1,alice";
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(mixed.as_bytes());

            let headers: Vec<String> = reader
                .headers()
                .unwrap()
                .iter()
                .map(|s| s.to_string())
                .collect();

            assert_eq!(headers[0], "id");
            assert_eq!(headers[1], "name");
        }
    }

    mod write_command_tag {
        use super::*;

        #[test]
        fn parse_affected_rows_for_update() {
            let out = "UPDATE 1\n";
            assert_eq!(PostgresAdapter::parse_affected_rows(out), Some(1));
        }

        #[test]
        fn parse_affected_rows_for_delete() {
            let out = "DELETE 3\n";
            assert_eq!(PostgresAdapter::parse_affected_rows(out), Some(3));
        }

        #[test]
        fn parse_affected_rows_returns_count_for_select() {
            let out = "SELECT 1\n";
            assert_eq!(PostgresAdapter::parse_affected_rows(out), Some(1));
        }

        #[test]
        fn update_zero_rows_returns_zero() {
            assert_eq!(PostgresAdapter::parse_affected_rows("UPDATE 0"), Some(0));
        }

        #[test]
        fn delete_large_number_returns_correct_value() {
            assert_eq!(
                PostgresAdapter::parse_affected_rows("DELETE 1000000"),
                Some(1000000)
            );
        }

        #[test]
        fn invalid_format_returns_none() {
            assert_eq!(PostgresAdapter::parse_affected_rows("FOOBAR"), None);
            assert_eq!(PostgresAdapter::parse_affected_rows("UPDATE abc"), None);
            assert_eq!(PostgresAdapter::parse_affected_rows(""), None);
        }
    }

    mod execute_query_raw_command_tag {
        use crate::domain::CommandTag;
        use crate::infra::adapters::postgres::PostgresAdapter;

        fn dml_stdout_returns_command_tag(
            stdout: &str,
            expected_tag: CommandTag,
            expected_rows: usize,
        ) {
            let tag = PostgresAdapter::extract_command_tag(stdout);
            assert_eq!(tag.as_ref(), Some(&expected_tag));
            let rows = tag.as_ref().and_then(|t| t.affected_rows()).unwrap_or(0) as usize;
            assert_eq!(rows, expected_rows);
        }

        #[test]
        fn update_stdout_yields_update_tag() {
            dml_stdout_returns_command_tag("UPDATE 3\n", CommandTag::Update(3), 3);
        }

        #[test]
        fn delete_stdout_yields_delete_tag() {
            dml_stdout_returns_command_tag("DELETE 5\n", CommandTag::Delete(5), 5);
        }

        #[test]
        fn insert_stdout_yields_insert_tag() {
            dml_stdout_returns_command_tag("INSERT 0 7\n", CommandTag::Insert(7), 7);
        }

        #[test]
        fn create_table_stdout_yields_create_tag_zero_rows() {
            let tag = PostgresAdapter::extract_command_tag("CREATE TABLE\n");
            assert_eq!(tag, Some(CommandTag::Create("TABLE".to_string())));
            assert_eq!(tag.unwrap().affected_rows(), None);
        }

        #[test]
        fn csv_stdout_is_not_mistaken_for_command_tag() {
            // CSV data: last line "1,Alice" does not match any DML pattern
            let csv = "id,name\n1,Alice\n2,Bob\n";
            let tag = PostgresAdapter::extract_command_tag(csv);
            // Should be Other or None, never a DML/DDL variant
            let is_dml = tag.as_ref().is_some_and(|t| t.is_data_modifying());
            assert!(!is_dml, "CSV output should not be parsed as DML tag");
        }

        #[test]
        fn select_csv_last_line_is_not_mistaken_for_select_tag() {
            // psql --csv does NOT append "SELECT N" to output; last line is data
            let csv = "count\n42\n";
            let tag = PostgresAdapter::extract_command_tag(csv);
            assert_ne!(tag, Some(CommandTag::Select(42)));
        }

        // psql returns "SELECT n" for CREATE TABLE AS SELECT
        #[test]
        fn select_tag_captured_for_ctas() {
            let tag = PostgresAdapter::parse_command_tag("SELECT 5");
            assert_eq!(tag, Some(CommandTag::Select(5)));
            let passes = tag
                .as_ref()
                .map(|t| t.is_data_modifying() || matches!(t, CommandTag::Select(_)))
                .unwrap_or(false);
            assert!(passes);
        }

        // 0-row SELECT header-only CSV parses as Other, which the filter rejects
        #[test]
        fn empty_select_header_not_captured_by_filter() {
            let cases = ["id,name", "id,name,email", "count"];
            for input in cases {
                let tag = PostgresAdapter::parse_command_tag(input);
                let passes = tag
                    .as_ref()
                    .map(|t| t.is_data_modifying() || matches!(t, CommandTag::Select(_)))
                    .unwrap_or(false);
                assert!(
                    !passes,
                    "header '{input}' must not pass the command-tag filter"
                );
            }
        }
    }
}
