#[cfg(test)]
mod xml_tests {
    use super::super::*;

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

        assert!(matches!(result, Err(DbOperationError::QueryFailed(_))));
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
            Err(DbOperationError::QueryFailed(_))
        ));
        assert_eq!(buffer, b"<resultset><row></row></resultset>");
    }
}
