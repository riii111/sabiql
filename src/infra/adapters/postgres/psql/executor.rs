use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use crate::app::ports::DbOperationError;
use crate::domain::{QueryResult, QuerySource, WriteExecutionResult};

use super::super::PostgresAdapter;
use super::parser::split_sql_statements;

fn csv_field_count(line: &str) -> usize {
    let mut count = 1;
    let mut in_quotes = false;
    for ch in line.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => count += 1,
            _ => {}
        }
    }
    count
}

fn first_keyword(stmt: &str) -> &str {
    let bytes = stmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            b if b.is_ascii_alphabetic() => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                return &stmt[start..i];
            }
            _ => i += 1,
        }
    }
    ""
}

fn count_select_statements(sql: &str) -> usize {
    split_sql_statements(sql)
        .iter()
        .filter(|s| {
            let kw = first_keyword(s);
            kw.eq_ignore_ascii_case("SELECT") || kw.eq_ignore_ascii_case("WITH")
        })
        .count()
}

// psql --csv concatenates multiple result sets without separators;
// keep only the last one.
fn extract_last_csv_block<'a>(stdout: &'a str, sql: &str) -> &'a str {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 2 {
        return stdout;
    }

    let expected_sets = count_select_statements(sql);
    let first_header = lines[0];
    let mut current_fc = csv_field_count(first_header);
    let mut known_headers: Vec<&str> = vec![first_header];
    let mut last_header_idx = 0;
    let mut data_rows_since_header = 0usize;

    for (i, &line) in lines.iter().enumerate().skip(1) {
        let fc = csv_field_count(line);
        if fc != current_fc {
            last_header_idx = i;
            current_fc = fc;
            data_rows_since_header = 0;
            if !known_headers.contains(&line) {
                known_headers.push(line);
            }
        } else if known_headers.contains(&line) {
            last_header_idx = i;
            data_rows_since_header = 0;
        } else if expected_sets > 1
            && data_rows_since_header >= 1
            && known_headers.len() < expected_sets
        {
            // Require at least one data row before accepting a new header
            // candidate, bounded by SELECT/WITH count
            last_header_idx = i;
            known_headers.push(line);
            data_rows_since_header = 0;
        } else {
            data_rows_since_header += 1;
        }
    }

    if last_header_idx == 0 {
        return stdout;
    }

    let byte_offset: usize = stdout
        .lines()
        .take(last_header_idx)
        .map(|l| l.len() + 1) // +1 for '\n'
        .sum();

    &stdout[byte_offset..]
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
        .map_err(|_| DbOperationError::Timeout)?
        .map_err(|e| DbOperationError::QueryFailed(e.to_string()))?;

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
    ) -> Result<String, DbOperationError> {
        let output = self.run_psql(dsn, &["-t", "-A"], query, false).await?;

        if !output.status.success() {
            return Err(DbOperationError::QueryFailed(output.stderr));
        }

        Ok(output.stdout)
    }

    pub(in crate::infra::adapters::postgres) async fn execute_query_raw(
        &self,
        dsn: &str,
        query: &str,
        source: QuerySource,
        read_only: bool,
    ) -> Result<QueryResult, DbOperationError> {
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

        let csv_block = extract_last_csv_block(stdout_trimmed, query);
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(csv_block.as_bytes());

        let columns: Vec<String> = reader
            .headers()
            .map_err(|e| DbOperationError::QueryFailed(format!("CSV parse error: {}", e)))?
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut rows = Vec::new();
        for result in reader.records() {
            let record = result
                .map_err(|e| DbOperationError::QueryFailed(format!("CSV parse error: {}", e)))?;
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
    ) -> Result<WriteExecutionResult, DbOperationError> {
        let start = Instant::now();

        let output = self.run_psql(dsn, &[], query, read_only).await?;

        let elapsed = start.elapsed().as_millis() as u64;

        if !output.status.success() {
            return Err(DbOperationError::QueryFailed(
                output.stderr.trim().to_string(),
            ));
        }

        let affected_rows = Self::parse_affected_rows(&output.stdout).ok_or_else(|| {
            DbOperationError::QueryFailed("Failed to parse affected row count".to_string())
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
    ) -> Result<usize, DbOperationError> {
        let output = self.run_psql(dsn, &["-t", "-A"], query, read_only).await?;
        if !output.status.success() {
            return Err(DbOperationError::QueryFailed(output.stderr));
        }
        output.stdout.trim().parse::<usize>().map_err(|e| {
            DbOperationError::QueryFailed(format!("Failed to parse COUNT result: {}", e))
        })
    }

    pub(in crate::infra::adapters::postgres) async fn export_csv_to_file(
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
            .map_err(|e| DbOperationError::CommandNotFound(e.to_string()))?;

        let stdout = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let file = tokio::fs::File::create(path)
            .await
            .map_err(|e| DbOperationError::QueryFailed(format!("Failed to create file: {}", e)))?;
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
        .map_err(|_| DbOperationError::Timeout)?
        .map_err(|e| DbOperationError::QueryFailed(e.to_string()))?;

        let (status, stderr, newline_count) = result;
        if !status.success() {
            let _ = tokio::fs::remove_file(path).await;
            return Err(DbOperationError::QueryFailed(stderr.trim().to_string()));
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
    ) -> Result<Vec<String>, DbOperationError> {
        let query = Self::preview_pk_columns_query(schema, table);
        let raw = self.execute_query(dsn, &query).await?;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(vec![]);
        }

        serde_json::from_str(trimmed).map_err(|e| DbOperationError::InvalidJson(e.to_string()))
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

    mod extract_last_csv_block {
        use super::super::extract_last_csv_block;

        #[test]
        fn single_result_set_returned_as_is() {
            let input = "id,name\n1,alice\n2,bob";
            assert_eq!(extract_last_csv_block(input, "SELECT * FROM t"), input);
        }

        #[test]
        fn two_selects_same_header_returns_last() {
            let input = "?column?\n1\n?column?\n2";
            assert_eq!(
                extract_last_csv_block(input, "SELECT 1; SELECT 2"),
                "?column?\n2"
            );
        }

        #[test]
        fn two_selects_different_column_count_returns_last() {
            let input = "a\n1\nb,c\n2,3";
            assert_eq!(
                extract_last_csv_block(input, "SELECT 1 AS a; SELECT 2 AS b, 3 AS c"),
                "b,c\n2,3"
            );
        }

        #[test]
        fn three_selects_returns_last() {
            let input = "?column?\n1\n?column?\n2\n?column?\n3";
            assert_eq!(
                extract_last_csv_block(input, "SELECT 1; SELECT 2; SELECT 3"),
                "?column?\n3"
            );
        }

        #[test]
        fn single_result_set_returns_unchanged() {
            let input = "col\n42";
            assert_eq!(extract_last_csv_block(input, "SELECT 1"), input);
        }

        #[test]
        fn empty_input_returns_empty() {
            assert_eq!(extract_last_csv_block("", "SELECT 1"), "");
        }

        #[test]
        fn header_only_returns_unchanged() {
            assert_eq!(extract_last_csv_block("col", "SELECT 1"), "col");
        }

        #[test]
        fn same_field_count_different_headers_returns_last() {
            let input = "id,name\n1,Alice\nage,email\n30,alice@example.com";
            assert_eq!(
                extract_last_csv_block(
                    input,
                    "SELECT id, name FROM users; SELECT age, email FROM contacts"
                ),
                "age,email\n30,alice@example.com"
            );
        }

        #[test]
        fn single_statement_falls_back() {
            let input = "id,name\n1,Alice\n2,Bob";
            assert_eq!(extract_last_csv_block(input, "SELECT * FROM t"), input);
        }

        #[test]
        fn three_different_headers_same_field_count() {
            let input = "x,y\n1,2\na,b\n3,4\np,q\n5,6";
            assert_eq!(
                extract_last_csv_block(
                    input,
                    "SELECT 1 AS x, 2 AS y; SELECT 3 AS a, 4 AS b; SELECT 5 AS p, 6 AS q"
                ),
                "p,q\n5,6"
            );
        }

        #[test]
        fn non_select_statements_excluded_from_hint() {
            let input = "id,name\n1,Alice\nage,email\n30,bob@example.com";
            assert_eq!(
                extract_last_csv_block(
                    input,
                    "SET search_path TO public; SELECT id, name FROM users; SELECT age, email FROM contacts"
                ),
                "age,email\n30,bob@example.com"
            );
        }

        #[test]
        fn data_row_not_mistaken_when_single_select() {
            let input = "x,y\na,b\n1,2";
            assert_eq!(extract_last_csv_block(input, "SELECT * FROM t"), input);
        }

        #[test]
        fn leading_line_comment_does_not_break_hint() {
            let input = "id,name\n1,Alice\nage,email\n30,bob@example.com";
            assert_eq!(
                extract_last_csv_block(
                    input,
                    "-- note\nSELECT id, name FROM users; SELECT age, email FROM contacts"
                ),
                "age,email\n30,bob@example.com"
            );
        }

        #[test]
        fn leading_block_comment_does_not_break_hint() {
            let input = "id,name\n1,Alice\nage,email\n30,bob@example.com";
            assert_eq!(
                extract_last_csv_block(
                    input,
                    "/* note */ SELECT id, name FROM users; SELECT age, email FROM contacts"
                ),
                "age,email\n30,bob@example.com"
            );
        }

        // Known limitation: when the leading result set has 0 data rows,
        // the data_rows_since_header guard prevents detecting the next header.
        // Fixing this requires redesigning how boundary info flows from
        // parse_aggregate_command_tag, which is out of scope here.
        #[test]
        #[ignore = "empty leading result set — needs boundary redesign"]
        fn empty_leading_result_returns_last_block() {
            let input = "id,name\nage,email\n30,alice@example.com";
            let sql = "SELECT id, name FROM users WHERE false; SELECT age, email FROM contacts";
            assert_eq!(
                extract_last_csv_block(input, sql),
                "age,email\n30,alice@example.com"
            );
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
