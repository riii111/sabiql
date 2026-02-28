use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::MetadataError;
use crate::domain::{QueryResult, QuerySource, WriteExecutionResult};

use super::super::PostgresAdapter;

/// Raw output from a psql invocation.
struct PsqlOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl PostgresAdapter {
    /// Spawn psql with the given DSN, extra flags, and query, returning the
    /// raw stdout/stderr. All three `execute_*` methods delegate here.
    async fn run_psql(
        &self,
        dsn: &str,
        extra_args: &[&str],
        query: &str,
    ) -> Result<PsqlOutput, MetadataError> {
        let mut cmd = Command::new("psql");
        cmd.arg(dsn)
            .arg("-X") // Ignore .psqlrc to avoid unexpected output
            .arg("-v")
            .arg("ON_ERROR_STOP=1"); // Exit with non-zero on SQL errors

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.arg("-c").arg(query);

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true) // Ensure child process is killed on timeout/drop
            .spawn()
            .map_err(|e| MetadataError::CommandNotFound(e.to_string()))?;

        // Read stdout/stderr BEFORE wait() to prevent pipe buffer deadlock
        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let result = timeout(Duration::from_secs(self.timeout_secs), async {
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
        let output = self
            .run_psql(dsn, &["-t", "-A"], query) // Tuples only, unaligned output
            .await?;

        if !output.status.success() {
            return Err(MetadataError::QueryFailed(output.stderr));
        }

        Ok(output.stdout)
    }

    /// Execute a raw SQL query and return structured results.
    /// This is used for adhoc queries and preview queries.
    pub(in crate::infra::adapters::postgres) async fn execute_query_raw(
        &self,
        dsn: &str,
        query: &str,
        source: QuerySource,
    ) -> Result<QueryResult, MetadataError> {
        let start = Instant::now();

        let output = self
            .run_psql(dsn, &["--csv"], query) // CSV output format
            .await?;

        let elapsed = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            return Ok(QueryResult::error(
                query.to_string(),
                output.stderr.trim().to_string(),
                elapsed,
                source,
            ));
        }

        // Parse CSV output using csv crate for robust handling
        if output.stdout.trim().is_empty() {
            return Ok(QueryResult::success(
                query.to_string(),
                Vec::new(),
                Vec::new(),
                elapsed,
                source,
            ));
        }

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(output.stdout.as_bytes());

        // Get column headers
        let columns: Vec<String> = reader
            .headers()
            .map_err(|e| MetadataError::QueryFailed(format!("CSV parse error: {}", e)))?
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Parse data rows
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
    ) -> Result<WriteExecutionResult, MetadataError> {
        let start = Instant::now();

        let output = self.run_psql(dsn, &[], query).await?;

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
        stdout.lines().rev().find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() != 2 {
                return None;
            }
            match parts[0] {
                "UPDATE" | "DELETE" => parts[1].parse::<usize>().ok(),
                _ => None,
            }
        })
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

            let _ = reader.headers().unwrap();
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
        fn parse_affected_rows_returns_none_for_unknown_output() {
            let out = "SELECT 1\n";
            assert_eq!(PostgresAdapter::parse_affected_rows(out), None);
        }
    }
}
