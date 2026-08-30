pub(in crate::adapters::mysql) fn quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

pub(in crate::adapters::mysql) fn quote_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\0' => escaped.push_str("\\0"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{001a}' => escaped.push_str("\\Z"),
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            _ => escaped.push(character),
        }
    }
    format!("'{escaped}'")
}
