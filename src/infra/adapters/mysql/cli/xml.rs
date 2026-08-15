use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesRef, Event};

use crate::app::ports::outbound::DbOperationError;
use crate::domain::QueryValue;

use super::error::{
    classify_mysql_query_failure, has_mysql_cli_error, trace_mysql_error,
    write_mysql_transcript_line,
};

#[derive(Debug, PartialEq, Eq)]
pub(in crate::adapters::mysql) struct MysqlResultSet {
    pub(in crate::adapters::mysql) columns: Vec<String>,
    pub(in crate::adapters::mysql) values: Vec<Vec<QueryValue>>,
}
const MYSQL_RESULTSET_START: &[u8] = b"<resultset";

const MYSQL_RESULTSET_END: &[u8] = b"</resultset>";

#[derive(Debug, Default)]
pub(super) struct MysqlResultsetFrameScanner {
    resultset_start: Option<usize>,
    resultset_end: Option<usize>,
    resultset_start_cursor: usize,
    resultset_end_cursor: usize,
}

impl MysqlResultsetFrameScanner {
    pub(super) fn frame_start(&mut self, buffer: &[u8]) -> Option<usize> {
        let _ = self.frame_bounds(buffer);
        self.resultset_start
    }

    pub(super) fn frame_bounds(&mut self, buffer: &[u8]) -> Option<(usize, usize)> {
        if self.resultset_start_cursor > buffer.len() {
            self.resultset_start_cursor = 0;
        }
        if self.resultset_end_cursor > buffer.len() {
            self.resultset_end_cursor = 0;
        }

        if self.resultset_start.is_none() {
            let scan_start = self
                .resultset_start_cursor
                .saturating_sub(MYSQL_RESULTSET_START.len().saturating_sub(1));
            self.resultset_start = find_bytes_from(buffer, MYSQL_RESULTSET_START, scan_start);
            if let Some(start) = self.resultset_start {
                self.resultset_end_cursor = start;
            } else {
                self.resultset_start_cursor = buffer.len();
                return None;
            }
        }

        let start = self.resultset_start?;
        if self.resultset_end.is_none() {
            let scan_start = self
                .resultset_end_cursor
                .saturating_sub(MYSQL_RESULTSET_END.len().saturating_sub(1))
                .max(start);
            self.resultset_end = find_bytes_from(buffer, MYSQL_RESULTSET_END, scan_start)
                .map(|end| end + MYSQL_RESULTSET_END.len());
            if self.resultset_end.is_none() {
                self.resultset_end_cursor = buffer.len();
                return None;
            }
        }

        self.resultset_end.map(|end| (start, end))
    }

    #[cfg(any(not(unix), test))]
    fn take(&mut self, buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
        let bounds = self.frame_bounds(buffer)?;
        Some(self.take_bounds(buffer, bounds))
    }

    fn take_bounds(&mut self, buffer: &mut Vec<u8>, (start, end): (usize, usize)) -> Vec<u8> {
        let frame = buffer[start..end].to_vec();
        buffer.drain(..end);
        self.resultset_start_cursor = self
            .resultset_start_cursor
            .saturating_sub(end)
            .min(buffer.len());
        self.resultset_end_cursor = self
            .resultset_end_cursor
            .saturating_sub(end)
            .min(buffer.len());
        self.resultset_start = None;
        self.resultset_end = None;
        frame
    }

    #[cfg(unix)]
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(any(unix, test))]
pub(super) fn take_mysql_pty_resultset_frame(
    buffer: &mut Vec<u8>,
    scanner: &mut MysqlResultsetFrameScanner,
) -> Result<Option<Vec<u8>>, DbOperationError> {
    let bounds = scanner.frame_bounds(buffer);
    let resultset_start = scanner.resultset_start.unwrap_or(buffer.len());
    if has_mysql_cli_error(&buffer[..resultset_start]) {
        trace_mysql_error(&buffer[..resultset_start]);
        return Err(classify_mysql_query_failure(&buffer[..resultset_start]));
    }
    Ok(bounds.map(|bounds| scanner.take_bounds(buffer, bounds)))
}

#[cfg(any(not(unix), test))]
pub(super) fn take_mysql_resultset_frame_after_error_check(
    buffer: &mut Vec<u8>,
    error_output: &[u8],
    scanner: &mut MysqlResultsetFrameScanner,
) -> Result<Option<Vec<u8>>, DbOperationError> {
    if has_mysql_cli_error(error_output) {
        trace_mysql_error(error_output);
        return Err(classify_mysql_query_failure(error_output));
    }
    Ok(scanner.take(buffer))
}

fn find_bytes_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    haystack
        .get(start..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

pub(super) fn trace_mysql_frame(kind: &str, bytes: usize) {
    if std::env::var_os("SABIQL_MYSQL_TRANSCRIPT").is_some() {
        write_mysql_transcript_line(&format!("sabiql mysql frame: {kind}, bytes={bytes}"));
    }
}

pub(super) fn trace_mysql_statement(statement: &str) {
    if std::env::var_os("SABIQL_MYSQL_TRANSCRIPT").is_some() {
        write_mysql_transcript_line(&format!(
            "sabiql mysql stage: send statement, keyword={}, bytes={}",
            mysql_statement_keyword(statement),
            statement.len()
        ));
    }
}

fn mysql_statement_keyword(statement: &str) -> String {
    let keyword = statement
        .trim_start()
        .split(|character: char| !character.is_ascii_alphabetic())
        .next()
        .unwrap_or_default();
    if keyword.is_empty() {
        "unknown".to_string()
    } else {
        keyword
            .chars()
            .take(32)
            .flat_map(char::to_uppercase)
            .collect()
    }
}

pub(super) fn decode_mysql_xml_reference(
    reference: &BytesRef<'_>,
) -> Result<String, DbOperationError> {
    let reference = reference.decode().map_err(|error| {
        DbOperationError::QueryFailed(format!("invalid MySQL XML text: {error}"))
    })?;
    unescape(&format!("&{reference};"))
        .map(std::borrow::Cow::into_owned)
        .map_err(|error| DbOperationError::QueryFailed(format!("invalid MySQL XML text: {error}")))
}

pub(super) fn parse_mysql_xml(xml: &[u8]) -> Result<MysqlResultSet, DbOperationError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut resultset_count = 0;
    let mut in_resultset = false;
    let mut current_row: Option<Vec<(String, QueryValue)>> = None;
    let mut current_field: Option<MysqlField> = None;
    let mut rows = Vec::new();
    let mut columns = Vec::new();

    loop {
        let event = reader.read_event_into(&mut buffer).map_err(|error| {
            DbOperationError::QueryFailed(format!("invalid MySQL XML result: {error}"))
        })?;
        match event {
            Event::Start(element) => match element.name().as_ref() {
                b"resultset" => {
                    if in_resultset || resultset_count > 0 {
                        return Err(DbOperationError::QueryFailed(
                            "mysql returned more than one resultset".to_string(),
                        ));
                    }
                    resultset_count += 1;
                    in_resultset = true;
                }
                b"row" if in_resultset && current_row.is_none() => {
                    current_row = Some(Vec::new());
                }
                b"field" if current_row.is_some() && current_field.is_none() => {
                    current_field = Some(parse_mysql_field(&element)?);
                }
                _ => {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected element in MySQL XML result".to_string(),
                    ));
                }
            },
            Event::Empty(element) if element.name().as_ref() == b"field" => {
                let row = current_row.as_mut().ok_or_else(|| {
                    DbOperationError::QueryFailed("MySQL XML field is outside a row".to_string())
                })?;
                if current_field.is_some() {
                    return Err(DbOperationError::QueryFailed(
                        "nested MySQL XML fields are not supported".to_string(),
                    ));
                }
                let field = parse_mysql_field(&element)?;
                row.push(field.finish());
            }
            Event::Text(text) => {
                let decoded = text.decode().map_err(|error| {
                    DbOperationError::QueryFailed(format!("invalid MySQL XML text: {error}"))
                })?;
                let text = unescape(&decoded).map_err(|error| {
                    DbOperationError::QueryFailed(format!("invalid MySQL XML text: {error}"))
                })?;
                if let Some(field) = current_field.as_mut() {
                    field.value.push_str(&text);
                } else if !text.chars().all(char::is_whitespace) {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected text in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::GeneralRef(reference) => {
                let text = decode_mysql_xml_reference(&reference)?;
                if let Some(field) = current_field.as_mut() {
                    field.value.push_str(&text);
                } else if !text.chars().all(char::is_whitespace) {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected text in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::CData(data) => {
                if let Some(field) = current_field.as_mut() {
                    field
                        .value
                        .push_str(std::str::from_utf8(data.as_ref()).map_err(|error| {
                            DbOperationError::QueryFailed(format!(
                                "invalid MySQL XML text: {error}"
                            ))
                        })?);
                } else {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected CDATA in MySQL XML result".to_string(),
                    ));
                }
            }
            Event::End(element) => match element.name().as_ref() {
                b"field" => {
                    let row = current_row.as_mut().ok_or_else(|| {
                        DbOperationError::QueryFailed(
                            "MySQL XML field is outside a row".to_string(),
                        )
                    })?;
                    let field = current_field.take().ok_or_else(|| {
                        DbOperationError::QueryFailed("unexpected MySQL XML field end".to_string())
                    })?;
                    row.push(field.finish());
                }
                b"row" => {
                    let row = current_row.take().ok_or_else(|| {
                        DbOperationError::QueryFailed("unexpected MySQL XML row end".to_string())
                    })?;
                    if columns.is_empty() {
                        columns = row.iter().map(|(name, _)| name.clone()).collect();
                    } else if row.len() != columns.len()
                        || row
                            .iter()
                            .zip(&columns)
                            .any(|((name, _), column)| name != column)
                    {
                        return Err(DbOperationError::QueryFailed(
                            "MySQL XML rows have inconsistent fields".to_string(),
                        ));
                    }
                    rows.push(row.into_iter().map(|(_, value)| value).collect());
                }
                b"resultset" => {
                    if !in_resultset || current_row.is_some() || current_field.is_some() {
                        return Err(DbOperationError::QueryFailed(
                            "malformed MySQL XML resultset".to_string(),
                        ));
                    }
                    in_resultset = false;
                }
                _ => {
                    return Err(DbOperationError::QueryFailed(
                        "unexpected MySQL XML closing element".to_string(),
                    ));
                }
            },
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Eof => break,
            _ => {
                return Err(DbOperationError::QueryFailed(
                    "unexpected event in MySQL XML result".to_string(),
                ));
            }
        }
        buffer.clear();
    }

    if resultset_count != 1 || in_resultset || current_row.is_some() || current_field.is_some() {
        return Err(DbOperationError::QueryFailed(
            "MySQL XML result did not contain one complete resultset".to_string(),
        ));
    }
    Ok(MysqlResultSet {
        columns,
        values: rows,
    })
}

pub(super) struct MysqlField {
    name: String,
    pub(super) value: String,
    is_null: bool,
}

impl MysqlField {
    fn finish(self) -> (String, QueryValue) {
        let value = if self.is_null {
            QueryValue::Null
        } else {
            QueryValue::Text(self.value)
        };
        (self.name, value)
    }

    pub(super) fn finish_raw(self) -> (String, String) {
        let value = if self.is_null {
            String::new()
        } else {
            self.value
        };
        (self.name, value)
    }
}

pub(super) fn parse_mysql_field(
    element: &quick_xml::events::BytesStart<'_>,
) -> Result<MysqlField, DbOperationError> {
    let mut name = None;
    let mut is_null = false;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| {
            DbOperationError::QueryFailed(format!("invalid MySQL XML field: {error}"))
        })?;
        let value = attribute.unescape_value().map_err(|error| {
            DbOperationError::QueryFailed(format!("invalid MySQL XML field: {error}"))
        })?;
        match attribute.key.as_ref() {
            b"name" => name = Some(value.into_owned()),
            b"xsi:nil" | b"nil" => is_null = matches!(value.as_ref(), "true" | "1"),
            _ => {}
        }
    }
    let name = name
        .ok_or_else(|| DbOperationError::QueryFailed("MySQL XML field has no name".to_string()))?;
    Ok(MysqlField {
        name,
        value: String::new(),
        is_null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mysql_xml_without_collapsing_value_boundaries() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<row>
  <field name="null" xsi:nil="true"/>
  <field name="empty"></field>
  <field name="text-null">NULL</field>
  <field name="special">tab&#9;line&#10;slash\unicode 日本語</field>
  <field name="json">{"a":[1,true]}</field>
  <field name="binary-looking">0x41</field>
</row>
</resultset>"#;

        let result = parse_mysql_xml(xml.as_bytes()).unwrap();

        assert_eq!(
            result.columns,
            vec![
                "null",
                "empty",
                "text-null",
                "special",
                "json",
                "binary-looking"
            ]
        );
        assert_eq!(
            result.values,
            vec![vec![
                QueryValue::Null,
                QueryValue::Text(String::new()),
                QueryValue::Text("NULL".to_string()),
                QueryValue::Text("tab\tline\nslash\\unicode 日本語".to_string()),
                QueryValue::Text("{\"a\":[1,true]}".to_string()),
                QueryValue::Text("0x41".to_string()),
            ]]
        );
    }

    #[test]
    fn parses_numeric_and_binary_values_as_text() {
        let xml = br#"<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row>
<field name="integer">18446744073709551615</field>
<field name="decimal">12345678901234567890.123456789</field>
<field name="float">1.25e+100</field>
<field name="binary">0x00FF10</field>
</row></resultset>"#;

        let result = parse_mysql_xml(xml).unwrap();
        assert!(
            result.values[0]
                .iter()
                .all(|value| matches!(value, QueryValue::Text(_)))
        );
        assert_eq!(result.values[0][0].as_str(), Some("18446744073709551615"));
        assert_eq!(result.values[0][3].as_str(), Some("0x00FF10"));
    }

    #[test]
    fn rejects_multiple_resultsets_instead_of_guessing_the_last_one() {
        let xml = br#"<resultset><row><field name="value">1</field></row></resultset>
<resultset><row><field name="value">2</field></row></resultset>"#;

        assert!(matches!(
            parse_mysql_xml(xml),
            Err(DbOperationError::QueryFailed(details))
                if details.contains("more than one resultset")
        ));
    }

    #[test]
    fn accepts_xml_declaration_after_probe_separator_and_empty_resultsets() {
        let xml = br#"
<?xml version="1.0" encoding="utf-8"?>
<resultset></resultset>
"#;

        let result = parse_mysql_xml(xml).unwrap();
        assert!(result.columns.is_empty());
        assert!(result.values.is_empty());
    }
    #[test]
    fn frames_one_xml_resultset_and_preserves_following_output() {
        let mut buffer = b"    -> <?xml version=\"1.0\"?>\n<resultset></resultset>\r\n    -> <?xml version=\"1.0\"?>\n<resultset>"
            .to_vec();
        let mut scanner = MysqlResultsetFrameScanner::default();

        assert_eq!(
            scanner.take(&mut buffer),
            Some(b"<resultset></resultset>".to_vec())
        );
        assert_eq!(
            scanner.take(&mut buffer),
            None,
            "an incomplete following frame must remain buffered"
        );
        assert!(buffer.starts_with(b"\r\n    -> <?xml"));
    }

    #[test]
    fn frames_resultset_after_mysql_cli_text() {
        let mut buffer =
            b"SELECT 1;\n<?xml version=\"1.0\"?>\nquery text\n<resultset></resultset>".to_vec();
        let mut scanner = MysqlResultsetFrameScanner::default();

        assert_eq!(
            scanner.take(&mut buffer),
            Some(b"<resultset></resultset>".to_vec())
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn extracts_one_frame_when_end_delimiter_crosses_4k_chunk_boundary() {
        let delimiter_start = 4096 - 3;
        let mut expected = MYSQL_RESULTSET_START.to_vec();
        expected.resize(delimiter_start, b'x');
        expected.extend_from_slice(MYSQL_RESULTSET_END);

        let mut buffer = Vec::new();
        let mut scanner = MysqlResultsetFrameScanner::default();
        let mut frames = Vec::new();
        for chunk in expected.chunks(4096) {
            buffer.extend_from_slice(chunk);
            if let Some(frame) = scanner.take(&mut buffer) {
                frames.push(frame);
            }
        }

        assert_eq!(frames, vec![expected]);
        assert!(buffer.is_empty());
        assert_eq!(scanner.take(&mut buffer), None);
    }

    #[test]
    fn extracts_large_resultset_from_small_chunks() {
        let mut expected = MYSQL_RESULTSET_START.to_vec();
        expected.extend_from_slice(b"<row><field name=\"value\">");
        expected.extend(vec![b'x'; 128 * 1024]);
        expected.extend_from_slice(b"</field></row>");
        expected.extend_from_slice(MYSQL_RESULTSET_END);

        let mut buffer = Vec::new();
        let mut scanner = MysqlResultsetFrameScanner::default();
        let mut frames = Vec::new();
        for chunk in expected.chunks(37) {
            buffer.extend_from_slice(chunk);
            if let Some(frame) = scanner.take(&mut buffer) {
                frames.push(frame);
            }
        }

        assert_eq!(frames, vec![expected]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn drains_frames_in_order_without_skipping_the_following_frame() {
        let first = b"<resultset><row><field name=\"value\">one</field></row></resultset>";
        let second = b"noise<resultset><row><field name=\"value\">two</field></row></resultset>";
        let mut input = first.to_vec();
        input.extend_from_slice(second);
        let mut buffer = Vec::new();
        let mut scanner = MysqlResultsetFrameScanner::default();
        let mut frames = Vec::new();

        for chunk in input.chunks(11) {
            buffer.extend_from_slice(chunk);
            while let Some(frame) = scanner.take(&mut buffer) {
                frames.push(frame);
            }
        }

        assert_eq!(frames, vec![first.to_vec(), second[5..].to_vec()]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn delimiter_prefix_in_field_text_does_not_end_the_frame() {
        let expected = b"<resultset><row><field name=\"value\">literal </resultset prefix</field></row></resultset>";
        let mut buffer = expected.to_vec();
        let mut scanner = MysqlResultsetFrameScanner::default();

        assert_eq!(scanner.take(&mut buffer), Some(expected.to_vec()));
        assert!(buffer.is_empty());
    }

    #[test]
    fn resultset_field_error_text_is_not_classified_as_cli_error() {
        let mut buffer = br#"<resultset><row><field name="message">line 1
ERROR 1146 (42S02): this is a cell value</field></row></resultset>"#
            .to_vec();
        let mut scanner = MysqlResultsetFrameScanner::default();

        let frame = take_mysql_pty_resultset_frame(&mut buffer, &mut scanner).unwrap();

        assert!(frame.is_some());
        assert!(buffer.is_empty());
    }

    #[test]
    fn cli_error_before_resultset_frame_is_still_rejected() {
        let mut buffer =
            b"ERROR 1054 (42S22): Unknown column\n<resultset><row></row></resultset>".to_vec();
        let mut scanner = MysqlResultsetFrameScanner::default();

        let result = take_mysql_pty_resultset_frame(&mut buffer, &mut scanner);

        assert!(matches!(result, Err(DbOperationError::ObjectMissing(_))));
        assert_eq!(
            buffer,
            b"ERROR 1054 (42S22): Unknown column\n<resultset><row></row></resultset>"
        );
    }

    #[test]
    fn error_before_resultset_frame_is_not_accepted() {
        let mut buffer = b"<resultset><row></row></resultset>".to_vec();
        let error = b"ERROR 1054 (42S22): Unknown column missing_column\n";
        let mut scanner = MysqlResultsetFrameScanner::default();

        assert!(matches!(
            take_mysql_resultset_frame_after_error_check(&mut buffer, error, &mut scanner),
            Err(DbOperationError::ObjectMissing(_))
        ));
        assert_eq!(buffer, b"<resultset><row></row></resultset>");
    }
}
