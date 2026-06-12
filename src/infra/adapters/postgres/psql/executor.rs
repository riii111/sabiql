use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{CommandTag, QueryResult, QuerySource, WriteExecutionResult};

use super::super::PostgresAdapter;
use super::error::classify_query_error;
use super::parser::{ParseCommandTagError, split_sql_statements};

// Keep user SQL server-side: stdin scripts would let psql interpret
// line-leading backslash metacommands before the server sees them.

fn boundary_marker() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "__sabiql_boundary_{}_{}_{}__",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

// Match the implicit all-or-nothing behavior of a single multi-statement -c.
fn segmented_query_args(statements: &[&str], marker: &str) -> Vec<String> {
    let echo = format!("\\echo {marker}");
    let mut args = Vec::with_capacity(statements.len() * 4 + 1);
    args.push("--single-transaction".to_string());
    for stmt in statements {
        args.push("-c".to_string());
        args.push(echo.clone());
        args.push("-c".to_string());
        args.push((*stmt).to_string());
    }
    args
}

fn split_marker_segments<'a>(stdout: &'a str, marker: &str) -> Vec<&'a str> {
    let mut segments = Vec::new();
    let mut seg_start: Option<usize> = None;
    let mut offset = 0;
    for line in stdout.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == marker {
            if let Some(start) = seg_start {
                segments.push(stdout[start..offset].trim_matches(['\n', '\r']));
            }
            seg_start = Some(offset + line.len());
        }
        offset += line.len();
    }
    if let Some(start) = seg_start {
        segments.push(stdout[start..].trim_matches(['\n', '\r']));
    }
    segments
}

fn select_result_segment<'a>(segments: &[&'a str]) -> Option<&'a str> {
    segments
        .iter()
        .rev()
        .find(|seg| !seg.trim().is_empty() && !PostgresAdapter::is_command_tags_only(seg))
        .copied()
}

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
    ) -> Result<PsqlOutput, DbOperationError> {
        self.run_psql_args(dsn, extra_args, &["-c", query], read_only)
            .await
    }

    async fn run_psql_args(
        &self,
        dsn: &str,
        extra_args: &[&str],
        query_args: &[&str],
        read_only: bool,
    ) -> Result<PsqlOutput, DbOperationError> {
        let mut cmd = Command::new("psql");
        if read_only {
            Self::apply_read_only_pgoptions(&mut cmd);
        }
        Self::apply_psql_base_args(&mut cmd, dsn);

        for arg in extra_args {
            cmd.arg(arg);
        }
        for arg in query_args {
            cmd.arg(arg);
        }

        Self::collect_output(&mut cmd, self.timeout_secs).await
    }

    fn apply_read_only_pgoptions(cmd: &mut Command) {
        let merged = match std::env::var("PGOPTIONS") {
            Ok(existing) => format!("{} {}", Self::PGOPTIONS_READ_ONLY, existing),
            Err(_) => Self::PGOPTIONS_READ_ONLY.to_string(),
        };
        cmd.env("PGOPTIONS", merged);
    }

    fn apply_psql_base_args(cmd: &mut Command, dsn: &str) {
        cmd.arg(dsn)
            .arg("-X")
            .arg("-v")
            .arg("ON_ERROR_STOP=1")
            .arg("-v")
            .arg("VERBOSITY=verbose")
            .arg("-v")
            .arg("SHOW_CONTEXT=never");
    }

    async fn collect_output(
        cmd: &mut Command,
        timeout_secs: u64,
    ) -> Result<PsqlOutput, DbOperationError> {
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| DbOperationError::CommandNotFound(e.to_string()))?;

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
        .map_err(|e| DbOperationError::Timeout(e.to_string()))?
        .map_err(|e| DbOperationError::QueryFailed(e.to_string()))?;

        let (status, stdout, stderr) = result;
        Ok(PsqlOutput {
            status,
            stdout,
            stderr,
        })
    }

    pub(in crate::adapters::postgres) async fn execute_query(
        &self,
        dsn: &str,
        query: &str,
    ) -> Result<String, DbOperationError> {
        let output = self.run_psql(dsn, &["-t", "-A"], query, false).await?;

        if !output.status.success() {
            return Err(Self::classify_psql_error(&output.stderr));
        }

        Ok(output.stdout)
    }

    pub(in crate::adapters::postgres) async fn execute_query_raw(
        &self,
        dsn: &str,
        query: &str,
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        let statements = split_sql_statements(query);
        if statements.len() <= 1 {
            return self
                .execute_single_statement(dsn, query, source, read_only)
                .await;
        }
        self.execute_segmented_statements(dsn, query, &statements, source, read_only)
            .await
    }

    // Keep non-transaction-capable statements outside the segmented
    // --single-transaction path.
    async fn execute_single_statement(
        &self,
        dsn: &str,
        query: &str,
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures psql execution time at the I/O boundary"
        )]
        let start = Instant::now();

        let output = self.run_psql(dsn, &["--csv"], query, read_only).await?;

        let elapsed = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            return Err(Self::classify_psql_error(&output.stderr));
        }

        let stdout_trimmed = output.stdout.trim();
        if stdout_trimmed.is_empty() {
            return Ok(QueryResult::success(
                query.to_string(),
                Vec::new(),
                Vec::new(),
                elapsed,
                source,
            ));
        }

        if let Some(tag) = Self::parse_aggregate_command_tag(stdout_trimmed, query) {
            return Ok(Self::command_tag_result(query, tag, elapsed, source));
        }

        Self::csv_result(query, stdout_trimmed, elapsed, source)
    }

    async fn execute_segmented_statements(
        &self,
        dsn: &str,
        query: &str,
        statements: &[&str],
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures psql execution time at the I/O boundary"
        )]
        let start = Instant::now();

        let marker = boundary_marker();
        let args = segmented_query_args(statements, &marker);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .run_psql_args(dsn, &["--csv"], &arg_refs, read_only)
            .await?;

        let elapsed = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            return Err(Self::classify_psql_error(&output.stderr));
        }

        let segments = split_marker_segments(&output.stdout, &marker);
        // A mismatch implies a marker collision in data; guessing would
        // reintroduce silent result-set misattribution.
        if segments.len() != statements.len() {
            return Err(DbOperationError::QueryFailed(format!(
                "result-set boundary mismatch: expected {} segments, found {}",
                statements.len(),
                segments.len()
            )));
        }

        if let Some(csv_block) = select_result_segment(&segments) {
            return Self::csv_result(query, csv_block, elapsed, source);
        }

        let tags = segments.join("\n");
        if let Some(tag) = Self::parse_aggregate_command_tag(tags.trim(), query) {
            return Ok(Self::command_tag_result(query, tag, elapsed, source));
        }

        Ok(QueryResult::success(
            query.to_string(),
            Vec::new(),
            Vec::new(),
            elapsed,
            source,
        ))
    }

    fn command_tag_result(
        query: &str,
        tag: CommandTag,
        elapsed: u64,
        source: QuerySource,
    ) -> QueryResult {
        let row_count = tag.affected_rows().unwrap_or(0) as usize;
        let mut result =
            QueryResult::success(query.to_string(), Vec::new(), Vec::new(), elapsed, source);
        result.row_count = row_count;
        result.with_command_tag(tag)
    }

    fn csv_result(
        query: &str,
        csv_block: &str,
        elapsed: u64,
        source: QuerySource,
    ) -> Result<QueryResult, DbOperationError> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(csv_block.as_bytes());

        let columns: Vec<String> = reader.headers()?.iter().map(ToString::to_string).collect();

        let mut rows = Vec::new();
        for result in reader.records() {
            let record = result?;
            let row: Vec<String> = record.iter().map(ToString::to_string).collect();
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

    pub(in crate::adapters::postgres) async fn execute_write_raw(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<WriteExecutionResult, DbOperationError> {
        #[expect(
            clippy::disallowed_methods,
            reason = "infra measures psql execution time at the I/O boundary"
        )]
        let start = Instant::now();

        let output = self.run_psql(dsn, &[], query, read_only).await?;

        let elapsed = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            return Err(Self::classify_psql_error(&output.stderr));
        }

        let affected_rows = Self::parse_affected_rows_with_source(&output.stdout).map_err(
            |error: ParseCommandTagError| {
                DbOperationError::CommandTagParseFailed(error.to_string())
            },
        )?;

        Ok(WriteExecutionResult {
            affected_rows,
            execution_time_ms: elapsed,
        })
    }

    pub(in crate::adapters::postgres) async fn count_rows(
        &self,
        dsn: &str,
        query: &str,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        let output = self.run_psql(dsn, &["-t", "-A"], query, read_only).await?;
        if !output.status.success() {
            return Err(Self::classify_psql_error(&output.stderr));
        }
        output.stdout.trim().parse::<usize>().map_err(|e| {
            DbOperationError::QueryFailed(format!("Failed to parse COUNT result: {e}"))
        })
    }

    pub(in crate::adapters::postgres) async fn export_csv_to_file(
        &self,
        dsn: &str,
        query: &str,
        path: &std::path::Path,
        read_only: bool,
    ) -> Result<usize, DbOperationError> {
        let mut cmd = Command::new("psql");
        if read_only {
            Self::apply_read_only_pgoptions(&mut cmd);
        }
        Self::apply_psql_base_args(&mut cmd, dsn);
        cmd.arg("--csv").arg("-c").arg(query);

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| DbOperationError::CommandNotFound(e.to_string()))?;

        let stdout = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let file = tokio::fs::File::create(path)
            .await
            .map_err(|e| DbOperationError::QueryFailed(format!("Failed to create file: {e}")))?;
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
        .await;

        let result = match result {
            Ok(inner) => inner.map_err(|e| DbOperationError::QueryFailed(e.to_string()))?,
            Err(e) => {
                let _ = tokio::fs::remove_file(path).await;
                return Err(DbOperationError::Timeout(e.to_string()));
            }
        };

        let (status, stderr, newline_count) = result;
        if !status.success() {
            let _ = tokio::fs::remove_file(path).await;
            return Err(Self::classify_psql_error(&stderr));
        }

        // Subtract 1 for the CSV header line
        let row_count = newline_count.saturating_sub(1);
        Ok(row_count)
    }

    pub(in crate::adapters::postgres) async fn fetch_preview_order_columns(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Vec<String>, DbOperationError> {
        let query = Self::preview_pk_columns_query(schema, table);
        let raw = self.execute_query(dsn, &query).await?;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(vec![]);
        }

        serde_json::from_str(trimmed).map_err(Into::into)
    }

    fn parse_affected_rows_with_source(stdout: &str) -> Result<usize, ParseCommandTagError> {
        let tag = Self::parse_command_tag(stdout)?;
        tag.affected_rows()
            .map(|n| n as usize)
            .ok_or_else(|| ParseCommandTagError::Invalid {
                input: format!("{tag:?}"),
            })
    }

    #[cfg(test)]
    pub(in crate::adapters::postgres) fn parse_affected_rows(stdout: &str) -> Option<usize> {
        Self::parse_affected_rows_with_source(stdout).ok()
    }

    fn classify_psql_error(stderr: &str) -> DbOperationError {
        classify_query_error(stderr)
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::postgres::PostgresAdapter;

    mod boundary_marker {
        use super::super::boundary_marker;

        #[test]
        fn consecutive_markers_are_unique() {
            assert_ne!(boundary_marker(), boundary_marker());
        }
    }

    mod segmented_query_args {
        use super::super::segmented_query_args;

        #[test]
        fn interleaves_echo_markers_inside_single_transaction() {
            let args = segmented_query_args(&["SELECT 1", "SELECT 2"], "M");

            assert_eq!(
                args,
                vec![
                    "--single-transaction",
                    "-c",
                    "\\echo M",
                    "-c",
                    "SELECT 1",
                    "-c",
                    "\\echo M",
                    "-c",
                    "SELECT 2",
                ]
            );
        }
    }

    mod split_marker_segments {
        use super::super::split_marker_segments;

        #[test]
        fn splits_segments_between_markers() {
            let stdout = "M\na\n1\nM\nb,c\n2,3\n";
            assert_eq!(split_marker_segments(stdout, "M"), vec!["a\n1", "b,c\n2,3"]);
        }

        #[test]
        fn drops_text_before_first_marker() {
            let stdout = "noise\nM\na\n1\n";
            assert_eq!(split_marker_segments(stdout, "M"), vec!["a\n1"]);
        }

        #[test]
        fn no_marker_returns_empty() {
            assert!(split_marker_segments("a\n1\n", "M").is_empty());
        }

        #[test]
        fn consecutive_markers_yield_empty_segment() {
            let stdout = "M\nM\nb\n2\n";
            assert_eq!(split_marker_segments(stdout, "M"), vec!["", "b\n2"]);
        }

        #[test]
        fn crlf_marker_line_is_recognized() {
            let stdout = "M\r\na\r\n1\r\nM\r\nb\r\n2\r\n";
            assert_eq!(split_marker_segments(stdout, "M"), vec!["a\r\n1", "b\r\n2"]);
        }

        #[test]
        fn data_line_containing_marker_substring_is_not_a_boundary() {
            let stdout = "M\nx\nprefix M suffix\nM\ny\n1\n";
            assert_eq!(
                split_marker_segments(stdout, "M"),
                vec!["x\nprefix M suffix", "y\n1"]
            );
        }
    }

    mod select_result_segment {
        use super::super::select_result_segment;

        #[test]
        fn tag_then_csv_returns_csv() {
            let segments = vec!["UPDATE 3", "id,name\n1,Alice"];
            assert_eq!(select_result_segment(&segments), Some("id,name\n1,Alice"));
        }

        #[test]
        fn csv_then_tag_returns_csv() {
            let segments = vec!["id,name\n1,Alice", "DELETE 2"];
            assert_eq!(select_result_segment(&segments), Some("id,name\n1,Alice"));
        }

        #[test]
        fn two_result_sets_returns_last() {
            let segments = vec!["a\n1", "b\n2"];
            assert_eq!(select_result_segment(&segments), Some("b\n2"));
        }

        #[test]
        fn tags_only_returns_none() {
            let segments = vec!["BEGIN", "UPDATE 1", "COMMIT"];
            assert_eq!(select_result_segment(&segments), None);
        }

        #[test]
        fn empty_leading_result_set_returns_last() {
            let segments = vec!["id,name", "age,email\n30,alice@example.com"];
            assert_eq!(
                select_result_segment(&segments),
                Some("age,email\n30,alice@example.com")
            );
        }

        #[test]
        fn empty_trailing_result_set_is_selected_over_earlier_data() {
            let segments = vec!["id,name\n1,Alice", "age,email"];
            assert_eq!(select_result_segment(&segments), Some("age,email"));
        }

        #[test]
        fn data_rows_identical_to_header_stay_in_segment() {
            let segments = vec!["id,name\n1,Alice", "id,name\nid,name"];
            assert_eq!(select_result_segment(&segments), Some("id,name\nid,name"));
        }

        #[test]
        fn blank_segments_are_skipped() {
            let segments = vec!["a\n1", ""];
            assert_eq!(select_result_segment(&segments), Some("a\n1"));
        }
    }

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
                .map(ToString::to_string)
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
                .map(ToString::to_string)
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
                .map(ToString::to_string)
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
                Some(1_000_000)
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
        use crate::adapters::postgres::PostgresAdapter;
        use crate::domain::CommandTag;

        fn dml_stdout_returns_command_tag(
            stdout: &str,
            expected_tag: CommandTag,
            expected_rows: usize,
        ) {
            let tag = PostgresAdapter::extract_command_tag(stdout);
            assert_eq!(tag.as_ref(), Some(&expected_tag));
            let rows = tag
                .as_ref()
                .and_then(crate::domain::command_tag::CommandTag::affected_rows)
                .unwrap_or(0) as usize;
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
            let is_dml = tag
                .as_ref()
                .is_some_and(crate::domain::command_tag::CommandTag::is_data_modifying);
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
            assert_eq!(tag, Ok(CommandTag::Select(5)));
            let passes = tag
                .ok()
                .as_ref()
                .is_some_and(|t| t.is_data_modifying() || matches!(t, CommandTag::Select(_)));
            assert!(passes);
        }

        // 0-row SELECT header-only CSV parses as Other, which the filter rejects
        #[test]
        fn empty_select_header_not_captured_by_filter() {
            let cases = ["id,name", "id,name,email", "count"];
            for input in cases {
                let tag = PostgresAdapter::parse_command_tag(input);
                let passes = tag
                    .ok()
                    .as_ref()
                    .is_some_and(|t| t.is_data_modifying() || matches!(t, CommandTag::Select(_)));
                assert!(
                    !passes,
                    "header '{input}' must not pass the command-tag filter"
                );
            }
        }
    }
}
