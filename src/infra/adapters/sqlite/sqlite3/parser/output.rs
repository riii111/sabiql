use std::collections::HashMap;

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{QueryResult, QuerySource, QueryValue};

use crate::adapters::sqlite::sql;

use super::lexer::{sqlite_probe_columns, sqlite_result_probe_columns};

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuotedRecord {
    offset: usize,
    values: Vec<QueryValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SplitSegment {
    offset: usize,
    text: String,
}

fn split_outside_sqlite_quotes(
    input: &str,
    delimiter: u8,
) -> Result<Vec<SplitSegment>, DbOperationError> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut in_quote = false;
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if in_quote && i + 1 < bytes.len() && bytes[i + 1] == b'\'' => {
                i += 2;
            }
            b'\'' => {
                in_quote = !in_quote;
                i += 1;
            }
            byte if byte == delimiter && !in_quote => {
                segments.push(SplitSegment {
                    offset: start,
                    text: input[start..i].to_string(),
                });
                start = i + 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if in_quote {
        return Err(DbOperationError::MetadataParseFailed(
            "unterminated SQLite quoted output".to_string(),
        ));
    }
    segments.push(SplitSegment {
        offset: start,
        text: input[start..].to_string(),
    });
    Ok(segments)
}

fn split_quoted_records(stdout: &str) -> Result<Vec<SplitSegment>, DbOperationError> {
    let mut records = split_outside_sqlite_quotes(stdout, b'\n')?
        .into_iter()
        .map(|segment| SplitSegment {
            offset: segment.offset,
            text: segment.text.trim_end_matches('\r').to_string(),
        })
        .collect::<Vec<_>>();
    records.retain(|segment| !segment.text.is_empty());
    Ok(records)
}

fn split_quoted_fields(record: &str) -> Result<Vec<String>, DbOperationError> {
    split_outside_sqlite_quotes(record, b',')
        .map(|segments| segments.into_iter().map(|segment| segment.text).collect())
}

fn unquote_sql_text(value: &str) -> String {
    value[1..value.len() - 1].replace("''", "'")
}

fn decode_hex_text(hex: &str) -> Result<String, DbOperationError> {
    let bytes = decode_hex_bytes(hex)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(in crate::adapters::sqlite) fn parse_unistr_inner_sql_escapes(
    value: &str,
) -> Result<String, DbOperationError> {
    let inner = value
        .strip_prefix("unistr(")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| {
            DbOperationError::MetadataParseFailed("invalid SQLite unistr literal".to_string())
        })?;
    let inner = inner
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .ok_or_else(|| {
            DbOperationError::MetadataParseFailed("invalid SQLite unistr literal".to_string())
        })?;

    let mut decoded = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                let next = chars.next().ok_or_else(|| {
                    DbOperationError::MetadataParseFailed(
                        "invalid SQLite unistr SQL string quote".to_string(),
                    )
                })?;
                if next != '\'' {
                    return Err(DbOperationError::MetadataParseFailed(
                        "invalid SQLite unistr SQL string quote".to_string(),
                    ));
                }
                decoded.push('\'');
            }
            '\\' => {
                let next = chars.next().ok_or_else(|| {
                    DbOperationError::MetadataParseFailed(
                        "invalid SQLite unistr escape sequence".to_string(),
                    )
                })?;
                if next == '\\' {
                    decoded.push('\\');
                } else {
                    decoded.push('\\');
                    decoded.push(next);
                }
            }
            ch => decoded.push(ch),
        }
    }
    Ok(decoded)
}

fn decode_sqlite_nul_text_transport(text: &str) -> Result<Option<String>, DbOperationError> {
    if let Some(hex) = text.strip_prefix(&sql::sqlite_nul_text_sentinel()) {
        return decode_hex_text(hex).map(Some);
    }
    if let Some(hex) = text.strip_prefix(sql::PREVIEW_TRANSPORT_UNISTR_PREFIX) {
        return decode_hex_text(hex).map(Some);
    }
    Ok(None)
}

pub(in crate::adapters::sqlite) fn decode_preview_transport_unistr(
    value: &str,
) -> Result<Option<String>, DbOperationError> {
    let inner = parse_unistr_inner_sql_escapes(value)?;
    decode_sqlite_nul_text_transport(&inner)
}

fn decode_hex_bytes(hex: &str) -> Result<Vec<u8>, DbOperationError> {
    if !hex.len().is_multiple_of(2) {
        return Err(DbOperationError::MetadataParseFailed(
            "invalid SQLite BLOB hex literal".to_string(),
        ));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut chars = hex.as_bytes().chunks_exact(2);
    for pair in &mut chars {
        let raw = std::str::from_utf8(pair)
            .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
        let byte = u8::from_str_radix(raw, 16)
            .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

pub(in crate::adapters::sqlite) fn parse_quoted_value(
    value: &str,
    source: QuerySource,
    decode_preview_transport: bool,
) -> Result<QueryValue, DbOperationError> {
    if value == "NULL" {
        return Ok(QueryValue::Null);
    }
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        let text = unquote_sql_text(value);
        if source == QuerySource::Preview
            && decode_preview_transport
            && let Some(decoded) = decode_sqlite_nul_text_transport(&text)?
        {
            return Ok(QueryValue::Text(decoded));
        }
        return Ok(QueryValue::Text(text));
    }
    if value.starts_with("unistr(") && value.ends_with(')') {
        if source == QuerySource::Preview
            && decode_preview_transport
            && let Some(text) = decode_preview_transport_unistr(value)?
        {
            return Ok(QueryValue::Text(text));
        }
        return Ok(QueryValue::SqlLiteral(value.to_string()));
    }
    if value.len() >= 3
        && value.as_bytes()[1] == b'\''
        && value.ends_with('\'')
        && value.as_bytes()[0].eq_ignore_ascii_case(&b'X')
    {
        return Ok(QueryValue::Blob(decode_hex_bytes(
            &value[2..value.len() - 1],
        )?));
    }
    if value == "Inf" {
        return Ok(QueryValue::SqlLiteral("1e999".to_string()));
    }
    if value == "-Inf" {
        return Ok(QueryValue::SqlLiteral("-1e999".to_string()));
    }
    Ok(QueryValue::SqlLiteral(value.to_string()))
}

fn parse_quoted_records(
    stdout: &str,
    source: QuerySource,
) -> Result<Vec<QuotedRecord>, DbOperationError> {
    split_quoted_records(stdout)?
        .into_iter()
        .enumerate()
        .map(|(index, segment)| {
            let decode_preview_transport = source == QuerySource::Preview && index > 0;
            split_quoted_fields(&segment.text)?
                .into_iter()
                .map(|field| parse_quoted_value(&field, source, decode_preview_transport))
                .collect::<Result<Vec<_>, _>>()
                .map(|values| QuotedRecord {
                    offset: segment.offset,
                    values,
                })
        })
        .collect()
}

pub(in crate::adapters::sqlite) fn quoted_to_query_result(
    query: &str,
    stdout: &str,
    source: QuerySource,
    execution_time_ms: u64,
) -> Result<QueryResult, DbOperationError> {
    let stdout = stdout.trim();
    if stdout.is_empty() {
        return Ok(QueryResult::success(
            query.to_string(),
            Vec::new(),
            Vec::new(),
            execution_time_ms,
            source,
        ));
    }

    let mut records = parse_quoted_records(stdout, source)?;
    let Some(header) = records.first() else {
        return Ok(QueryResult::success(
            query.to_string(),
            Vec::new(),
            Vec::new(),
            execution_time_ms,
            source,
        ));
    };
    let columns = header
        .values
        .iter()
        .map(QueryValue::display_value)
        .collect();
    let values = records.drain(1..).map(|record| record.values).collect();
    Ok(QueryResult::success_with_values(
        query.to_string(),
        columns,
        values,
        execution_time_ms,
        source,
    ))
}

pub(in crate::adapters::sqlite) fn last_sqlite_result_set(
    stdout: &str,
    marker: &str,
) -> Result<Option<String>, DbOperationError> {
    let (stmt_col, marker_col) = sqlite_result_probe_columns(marker);
    let raw_records = split_quoted_records(stdout)?;
    let records = parse_quoted_records(stdout, QuerySource::Adhoc)?;

    let mut last_result = None;
    let mut result_start = 0;
    let mut index = 0;
    while index < records.len() {
        let record = &records[index];
        if record.values.len() == 2
            && record.values[0].as_str() == Some(stmt_col.as_str())
            && record.values[1].as_str() == Some(marker_col.as_str())
        {
            let value = records.get(index + 1).ok_or_else(|| {
                DbOperationError::CommandTagParseFailed(
                    "missing SQLite result marker row".to_string(),
                )
            })?;
            let marker_value = value
                .values
                .get(1)
                .and_then(QueryValue::as_str)
                .ok_or_else(|| {
                    DbOperationError::CommandTagParseFailed(
                        "invalid SQLite result marker".to_string(),
                    )
                })?;
            if marker_value != marker {
                return Err(DbOperationError::CommandTagParseFailed(
                    "mismatched SQLite result marker".to_string(),
                ));
            }
            last_result = Some(
                raw_records[result_start..index]
                    .iter()
                    .map(|segment| segment.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            index += 2;
            result_start = index;
        } else {
            index += 1;
        }
    }

    Ok(last_result)
}

pub(in crate::adapters::sqlite) fn strip_sqlite_probes(
    stdout: &str,
    marker: &str,
) -> Result<(String, HashMap<usize, usize>), DbOperationError> {
    if stdout.trim().is_empty() {
        return Ok((String::new(), HashMap::new()));
    }

    let (stmt_col, changes_col) = sqlite_probe_columns(marker);
    let raw_records = split_quoted_records(stdout)?;
    let records = parse_quoted_records(stdout, QuerySource::Adhoc)?;

    let mut changes = HashMap::new();
    let mut kept = Vec::new();
    let mut removed_probe = false;
    let mut index = 0;
    while index < records.len() {
        let record = &records[index];
        if record.values.len() == 2
            && record.values[0].as_str() == Some(stmt_col.as_str())
            && record.values[1].as_str() == Some(changes_col.as_str())
        {
            removed_probe = true;
            let value = records.get(index + 1).ok_or_else(|| {
                DbOperationError::CommandTagParseFailed(
                    "missing SQLite statement probe row".to_string(),
                )
            })?;
            let stmt_index = value
                .values
                .first()
                .and_then(QueryValue::as_str)
                .and_then(|raw| raw.parse::<usize>().ok())
                .ok_or_else(|| {
                    DbOperationError::CommandTagParseFailed(
                        "invalid SQLite statement probe index".to_string(),
                    )
                })?;
            let affected_rows = value
                .values
                .get(1)
                .and_then(QueryValue::as_str)
                .and_then(|raw| raw.parse::<usize>().ok())
                .ok_or_else(|| {
                    DbOperationError::CommandTagParseFailed(
                        "invalid SQLite statement probe changes".to_string(),
                    )
                })?;
            changes.insert(stmt_index, affected_rows);
            index += 2;
        } else {
            kept.push(raw_records[index].text.clone());
            index += 1;
        }
    }

    if !removed_probe {
        return Ok((stdout.to_string(), changes));
    }

    Ok((kept.join("\n"), changes))
}

fn first_csv_cell(stdout: &str) -> Result<String, DbOperationError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(stdout.trim().as_bytes());
    let mut records = reader.records();
    let record = records
        .next()
        .transpose()?
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))?;
    record
        .get(0)
        .map(ToString::to_string)
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))
}

fn last_csv_cell(stdout: &str) -> Result<String, DbOperationError> {
    let line = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(line.as_bytes());
    let mut records = reader.records();
    let record = records
        .next()
        .transpose()?
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))?;
    record
        .get(0)
        .map(ToString::to_string)
        .ok_or_else(|| DbOperationError::EmptyResponse(stdout.to_string()))
}

pub(in crate::adapters::sqlite) fn parse_affected_rows(
    stdout: &str,
) -> Result<usize, DbOperationError> {
    last_csv_cell(stdout)
        .map_err(|error| match error {
            DbOperationError::EmptyResponse(_) => {
                DbOperationError::CommandTagParseFailed(stdout.to_string())
            }
            other => other,
        })?
        .parse::<usize>()
        .map_err(|error| DbOperationError::CommandTagParseFailed(error.to_string()))
}

pub(in crate::adapters::sqlite) fn parse_count_result(
    stdout: &str,
) -> Result<usize, DbOperationError> {
    first_csv_cell(stdout)
        .map_err(|error| match error {
            DbOperationError::EmptyResponse(_) => {
                DbOperationError::QueryFailed("Failed to parse COUNT result".to_string())
            }
            other => other,
        })?
        .parse::<usize>()
        .map_err(|error| {
            DbOperationError::QueryFailed(format!("Failed to parse COUNT result: {error}"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parsing {
        use super::*;

        #[test]
        fn quoted_to_query_result_preserves_newline_for_single_statement() {
            let quoted = "'body','marker'\n'line 1\nline 2','ok'\n";

            let result = quoted_to_query_result(
                "SELECT body, marker FROM notes",
                quoted,
                QuerySource::Adhoc,
                1,
            )
            .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows(),
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
        }

        #[test]
        fn last_sqlite_result_set_uses_marker_boundaries() {
            let marker = "probe";
            let sqlite_output_with_ignored_first_result_set = "'ignored'\n1\n'probe_result_stmt','probe_result_marker'\n0,'probe'\n'body','marker'\n'line 1\nline 2','ok'\n'probe_result_stmt','probe_result_marker'\n1,'probe'\n";

            let quoted =
                last_sqlite_result_set(sqlite_output_with_ignored_first_result_set, marker)
                    .unwrap()
                    .unwrap();
            let result = quoted_to_query_result(
                "SELECT 1 AS ignored; SELECT body, marker FROM notes",
                &quoted,
                QuerySource::Adhoc,
                1,
            )
            .unwrap();

            assert_eq!(result.columns, vec!["body", "marker"]);
            assert_eq!(
                result.rows(),
                vec![vec!["line 1\nline 2".to_string(), "ok".to_string()]]
            );
        }

        #[test]
        fn quoted_to_query_result_preserves_sqlite_value_kinds() {
            let quoted = "'a','b','c','d'\nNULL,'','NULL',X'00FF41'\n";

            let result =
                quoted_to_query_result("SELECT a, b, c, d FROM t", quoted, QuerySource::Adhoc, 1)
                    .unwrap();

            assert_eq!(
                result.rows(),
                vec![vec![
                    "NULL".to_string(),
                    String::new(),
                    "NULL".to_string(),
                    "BLOB (3 bytes) 00 FF 41".to_string()
                ]]
            );
            assert!(matches!(result.value_at(0, 0), Some(QueryValue::Null)));
            assert_eq!(
                result.value_at(0, 1),
                Some(&QueryValue::Text(String::new()))
            );
            assert_eq!(
                result.value_at(0, 2),
                Some(&QueryValue::Text("NULL".to_string()))
            );
            assert_eq!(
                result.value_at(0, 3),
                Some(&QueryValue::Blob(vec![0, 255, 65]))
            );
        }

        #[test]
        fn quoted_to_query_result_normalizes_infinite_numeric_literals() {
            let quoted = "'pos','neg'\nInf,-Inf\n";

            let result =
                quoted_to_query_result("SELECT 1e999, -1e999", quoted, QuerySource::Adhoc, 1)
                    .unwrap();

            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::SqlLiteral("1e999".to_string()))
            );
            assert_eq!(
                result.value_at(0, 1),
                Some(&QueryValue::SqlLiteral("-1e999".to_string()))
            );
        }

        #[test]
        fn quoted_to_query_result_rejects_unterminated_quote() {
            let result = quoted_to_query_result(
                "SELECT body FROM notes",
                "'body'\n'unclosed\nnext",
                QuerySource::Adhoc,
                1,
            );

            assert!(matches!(
                result,
                Err(DbOperationError::MetadataParseFailed(message))
                    if message == "unterminated SQLite quoted output"
            ));
        }

        #[test]
        fn parse_unistr_inner_sql_escapes_does_not_decode_unicode_sequences() {
            assert_eq!(
                parse_unistr_inner_sql_escapes("unistr('\\u0001\\u0001')").unwrap(),
                "\\u0001\\u0001"
            );
            assert_eq!(
                parse_unistr_inner_sql_escapes("unistr('\\u0001O''Reilly')").unwrap(),
                "\\u0001O'Reilly"
            );
        }

        #[test]
        fn decode_preview_transport_unistr_decodes_hex_payload() {
            assert_eq!(
                decode_preview_transport_unistr("unistr('\\u0001SABIQL_HEX:61006263')")
                    .unwrap()
                    .as_deref(),
                Some("a\0bc")
            );
            assert_eq!(
                decode_preview_transport_unistr("unistr('\\u0001SABIQL_HEX:0101')")
                    .unwrap()
                    .as_deref(),
                Some("\x01\x01")
            );
            assert_eq!(
                decode_preview_transport_unistr("unistr('\\u0001SABIQL_HEX:015C7530303031')")
                    .unwrap()
                    .as_deref(),
                Some("\x01\\u0001")
            );
        }

        #[test]
        fn parse_quoted_value_keeps_unrecoverable_adhoc_unistr_as_sql_literal() {
            assert_eq!(
                parse_quoted_value("unistr('\\u0001\\u0001')", QuerySource::Adhoc, true).unwrap(),
                QueryValue::SqlLiteral("unistr('\\u0001\\u0001')".to_string())
            );
            assert_eq!(
                parse_quoted_value("unistr('\\u0001O''Reilly')", QuerySource::Adhoc, true).unwrap(),
                QueryValue::SqlLiteral("unistr('\\u0001O''Reilly')".to_string())
            );
        }

        #[test]
        fn parse_quoted_value_decodes_preview_transport_unistr() {
            let value = parse_quoted_value(
                "unistr('\\u0001SABIQL_HEX:61006263')",
                QuerySource::Preview,
                true,
            )
            .unwrap();

            assert_eq!(value, QueryValue::Text("a\0bc".to_string()));
        }

        #[test]
        fn parse_quoted_value_decodes_preview_transport_plain_quoted() {
            let sentinel = sql::sqlite_nul_text_sentinel();
            let quoted = format!("'{sentinel}68656C6C6F'");
            let value = parse_quoted_value(&quoted, QuerySource::Preview, true).unwrap();

            assert_eq!(value, QueryValue::Text("hello".to_string()));
        }

        #[test]
        fn parse_quoted_value_keeps_plain_quoted_transport_as_text_for_adhoc() {
            let sentinel = sql::sqlite_nul_text_sentinel();
            let transport = format!("{sentinel}68656C6C6F");
            let quoted = format!("'{transport}'");
            let value = parse_quoted_value(&quoted, QuerySource::Adhoc, true).unwrap();

            assert_eq!(value, QueryValue::Text(transport));
        }

        #[test]
        fn parse_quoted_value_skips_preview_transport_decode_for_column_names() {
            let sentinel = sql::sqlite_nul_text_sentinel();
            let transport = format!("{sentinel}68656C6C6F");
            let quoted = format!("'{transport}'");
            let value = parse_quoted_value(&quoted, QuerySource::Preview, false).unwrap();

            assert_eq!(value, QueryValue::Text(transport));
        }

        #[test]
        fn quoted_to_query_result_keeps_transport_like_column_name() {
            let sentinel = sql::sqlite_nul_text_sentinel();
            let column = format!("{sentinel}4142");
            let data = format!("'{sentinel}68656C6C6F'");
            let quoted = format!("'{column}'\n{data}\n");
            let result =
                quoted_to_query_result("SELECT 1", &quoted, QuerySource::Preview, 1).unwrap();

            assert_eq!(result.columns, vec![column]);
            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::Text("hello".to_string()))
            );
        }

        #[test]
        fn quoted_to_query_result_keeps_unrecoverable_adhoc_unistr_as_sql_literal() {
            let quoted = "'value'\nunistr('\\u0001\\u0001')\n";

            let result =
                quoted_to_query_result("SELECT char(1) || char(1)", quoted, QuerySource::Adhoc, 1)
                    .unwrap();

            assert_eq!(
                result.value_at(0, 0),
                Some(&QueryValue::SqlLiteral(
                    "unistr('\\u0001\\u0001')".to_string()
                ))
            );
        }

        #[test]
        fn parse_affected_rows_reads_trailing_changes_cell() {
            assert_eq!(parse_affected_rows("changes()\n3\n").unwrap(), 3);
        }

        #[test]
        fn strip_sqlite_probes_removes_probe_result_sets() {
            let marker = "probe";
            let stdout = "'id','name'\n1,'Alice'\n'probe_stmt','probe_changes'\n0,2\n'value'\n42\n";

            let (filtered, changes) = strip_sqlite_probes(stdout, marker).unwrap();

            assert_eq!(changes.get(&0), Some(&2));
            assert_eq!(filtered, "'id','name'\n1,'Alice'\n'value'\n42");
        }

        #[test]
        fn parse_count_result_reads_first_result_cell() {
            assert_eq!(parse_count_result("COUNT(*)\n42\n").unwrap(), 42);
        }
    }
}
