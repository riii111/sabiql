#[cfg(test)]
mod export_tests {
    use std::fs;

    #[cfg(unix)]
    use tokio::io::AsyncWriteExt;

    use super::super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn streams_mysql_xml_rows_into_csv_without_binary_type_inference() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<resultset xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><row>
<field name="comma">a,b</field>
<field name="quote">a&quot;b</field>
<field name="newline"><![CDATA[line1
line2]]></field>
<field name="tab">a	b</field>
<field name="unicode">日本語</field>
<field name="null" xsi:nil="true"/>
<field name="empty"></field>
<field name="binary">0x00FF</field>
</row></resultset>"#;
        let (mut input, output) = tokio::io::duplex(32);
        let producer = tokio::spawn(async move {
            input.write_all(xml.as_bytes()).await.unwrap();
        });
        let directory = tempdir().unwrap();
        let path = directory.path().join("stream.csv");
        let mut csv_writer = CsvFileWriter::create(path.clone()).await.unwrap();
        let mut reader = Reader::from_reader(BufReader::new(output));
        reader.config_mut().trim_text(false);

        stream_mysql_xml_to_csv(&mut reader, &mut csv_writer)
            .await
            .unwrap();
        csv_writer.finish().await.unwrap();
        producer.await.unwrap();

        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "comma,quote,newline,tab,unicode,null,empty,binary\n\
             \"a,b\",\"a\"\"b\",\"line1\n\
             line2\",a\tb,日本語,,,0x00FF\n"
                .replace("             ", "")
        );
    }
}
