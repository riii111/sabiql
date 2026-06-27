use crate::app::ports::outbound::DbOperationError;
use crate::domain::{Trigger, TriggerEvent, TriggerTiming};

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
            } else {
                return i + 1;
            }
        } else {
            i += 1;
        }
    }
    i
}

fn skip_bracket_quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b']' {
            return i + 1;
        }
        i += 1;
    }
    i
}

fn next_keyword_from(sql: &str, mut i: usize) -> Option<(&str, usize)> {
    let bytes = sql.as_bytes();
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
            b'\'' | b'"' | b'`' => {
                i = skip_quoted(bytes, i, bytes[i]);
            }
            b'[' => {
                i = skip_bracket_quoted(bytes, i);
            }
            b if b.is_ascii_alphabetic() => {
                let start = i;
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
                return Some((&sql[start..i], i));
            }
            _ => i += 1,
        }
    }
    None
}

fn sqlite_trigger_parse_error(sql: &str, detail: &str) -> DbOperationError {
    DbOperationError::MetadataParseFailed(format!(
        "sqlite trigger parse failed ({detail}): {}",
        sql.chars().take(120).collect::<String>()
    ))
}

fn skip_optional_if_not_exists(sql: &str, pos: usize) -> usize {
    let Some((keyword, next)) = next_keyword_from(sql, pos) else {
        return pos;
    };
    if !keyword.eq_ignore_ascii_case("IF") {
        return pos;
    }
    let Some((not, next)) = next_keyword_from(sql, next) else {
        return pos;
    };
    if !not.eq_ignore_ascii_case("NOT") {
        return pos;
    }
    let Some((exists, next)) = next_keyword_from(sql, next) else {
        return pos;
    };
    if exists.eq_ignore_ascii_case("EXISTS") {
        next
    } else {
        pos
    }
}

fn skip_object_reference(sql: &str, pos: usize) -> usize {
    let bytes = sql.as_bytes();
    let mut i = pos;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = match bytes.get(i) {
        Some(b'"' | b'\'' | b'`') => skip_quoted(bytes, i, bytes[i]),
        Some(b'[') => skip_bracket_quoted(bytes, i),
        Some(b) if b.is_ascii_alphanumeric() || *b == b'_' => {
            let Some((_, end)) = next_keyword_from(sql, i) else {
                return i;
            };
            end
        }
        _ => return i,
    };
    if bytes.get(start) == Some(&b'.')
        && let Some((_, end)) = next_keyword_from(sql, start + 1)
    {
        return end;
    }
    start
}

fn skip_update_of_clause(sql: &str, pos: usize) -> usize {
    let Some((keyword, next)) = next_keyword_from(sql, pos) else {
        return pos;
    };
    if !keyword.eq_ignore_ascii_case("OF") {
        return pos;
    }

    let mut pos = next;
    loop {
        let Some((keyword, next)) = next_keyword_from(sql, pos) else {
            return pos;
        };
        match keyword.to_ascii_uppercase().as_str() {
            "INSERT" | "UPDATE" | "DELETE" | "ON" | "FOR" | "WHEN" | "BEGIN" => return pos,
            _ => pos = next,
        }
    }
}

fn parse_sqlite_trigger_events(
    sql: &str,
    pos: usize,
) -> Result<(Vec<TriggerEvent>, usize), DbOperationError> {
    let mut events = Vec::new();
    let mut pos = pos;
    loop {
        let Some((keyword, next)) = next_keyword_from(sql, pos) else {
            return Err(sqlite_trigger_parse_error(sql, "missing trigger event"));
        };
        if keyword.eq_ignore_ascii_case("ON") {
            break;
        }

        match keyword.to_ascii_uppercase().as_str() {
            "INSERT" => events.push(TriggerEvent::Insert),
            "UPDATE" => {
                events.push(TriggerEvent::Update);
                pos = skip_update_of_clause(sql, next);
                continue;
            }
            "DELETE" => events.push(TriggerEvent::Delete),
            _ => return Err(sqlite_trigger_parse_error(sql, "unsupported trigger event")),
        }
        pos = next;
    }

    if events.is_empty() {
        return Err(sqlite_trigger_parse_error(sql, "no trigger events"));
    }

    Ok((events, pos))
}

fn parse_sqlite_trigger_header(
    sql: &str,
    pos: usize,
) -> Result<(TriggerTiming, Vec<TriggerEvent>, usize), DbOperationError> {
    let Some((keyword, next)) = next_keyword_from(sql, pos) else {
        return Err(sqlite_trigger_parse_error(
            sql,
            "missing trigger timing or event",
        ));
    };

    match keyword.to_ascii_uppercase().as_str() {
        "BEFORE" => {
            let (events, pos) = parse_sqlite_trigger_events(sql, next)?;
            Ok((TriggerTiming::Before, events, pos))
        }
        "AFTER" => {
            let (events, pos) = parse_sqlite_trigger_events(sql, next)?;
            Ok((TriggerTiming::After, events, pos))
        }
        "INSTEAD" => {
            let Some((of, next)) = next_keyword_from(sql, next) else {
                return Err(sqlite_trigger_parse_error(sql, "incomplete INSTEAD OF"));
            };
            if !of.eq_ignore_ascii_case("OF") {
                return Err(sqlite_trigger_parse_error(sql, "expected OF after INSTEAD"));
            }
            let (events, pos) = parse_sqlite_trigger_events(sql, next)?;
            Ok((TriggerTiming::InsteadOf, events, pos))
        }
        "INSERT" | "UPDATE" | "DELETE" => {
            let (events, pos) = parse_sqlite_trigger_events(sql, pos)?;
            Ok((TriggerTiming::Before, events, pos))
        }
        _ => Err(sqlite_trigger_parse_error(
            sql,
            "unsupported trigger timing or event",
        )),
    }
}

pub(super) fn parse_sqlite_trigger(
    trigger_name: &str,
    sql: &str,
) -> Result<Trigger, DbOperationError> {
    let Some((first, pos)) = next_keyword_from(sql, 0) else {
        return Err(sqlite_trigger_parse_error(sql, "missing CREATE"));
    };
    if !first.eq_ignore_ascii_case("CREATE") {
        return Err(sqlite_trigger_parse_error(sql, "expected CREATE"));
    }
    let Some((second, mut pos)) = next_keyword_from(sql, pos) else {
        return Err(sqlite_trigger_parse_error(sql, "missing TRIGGER"));
    };
    if second.eq_ignore_ascii_case("TEMP") || second.eq_ignore_ascii_case("TEMPORARY") {
        let Some((third, next)) = next_keyword_from(sql, pos) else {
            return Err(sqlite_trigger_parse_error(sql, "missing TRIGGER"));
        };
        if !third.eq_ignore_ascii_case("TRIGGER") {
            return Err(sqlite_trigger_parse_error(sql, "expected TRIGGER"));
        }
        pos = next;
    } else if !second.eq_ignore_ascii_case("TRIGGER") {
        return Err(sqlite_trigger_parse_error(sql, "expected TRIGGER"));
    }
    pos = skip_optional_if_not_exists(sql, pos);
    pos = skip_object_reference(sql, pos);

    let (timing, events, _) = parse_sqlite_trigger_header(sql, pos)?;

    Ok(Trigger {
        name: trigger_name.to_string(),
        timing,
        events,
        function_name: sql.to_string(),
        security_definer: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_after_insert_trigger() {
        let sql = "CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END";
        let trigger = parse_sqlite_trigger("users_audit", sql).unwrap();

        assert_eq!(trigger.name, "users_audit");
        assert_eq!(trigger.timing, TriggerTiming::After);
        assert_eq!(trigger.events, vec![TriggerEvent::Insert]);
        assert_eq!(trigger.function_name, sql);
        assert!(!trigger.security_definer);
    }

    #[test]
    fn parses_before_update_of_columns() {
        let sql = "CREATE TRIGGER users_guard BEFORE UPDATE OF name ON users BEGIN SELECT 1; END";
        let trigger = parse_sqlite_trigger("users_guard", sql).unwrap();

        assert_eq!(trigger.timing, TriggerTiming::Before);
        assert_eq!(trigger.events, vec![TriggerEvent::Update]);
    }

    #[test]
    fn parses_instead_of_delete_trigger() {
        let sql =
            "CREATE TRIGGER users_view_io INSTEAD OF DELETE ON users_view BEGIN SELECT 1; END";
        let trigger = parse_sqlite_trigger("users_view_io", sql).unwrap();

        assert_eq!(trigger.timing, TriggerTiming::InsteadOf);
        assert_eq!(trigger.events, vec![TriggerEvent::Delete]);
    }

    #[test]
    fn omitted_timing_defaults_to_before() {
        let sql = "CREATE TRIGGER users_log INSERT ON users BEGIN SELECT 1; END";
        let trigger = parse_sqlite_trigger("users_log", sql).unwrap();

        assert_eq!(trigger.timing, TriggerTiming::Before);
        assert_eq!(trigger.events, vec![TriggerEvent::Insert]);
    }

    #[test]
    fn parses_temp_trigger() {
        let sql = "CREATE TEMP TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END";
        let trigger = parse_sqlite_trigger("users_audit", sql).unwrap();

        assert_eq!(trigger.name, "users_audit");
        assert_eq!(trigger.timing, TriggerTiming::After);
        assert_eq!(trigger.events, vec![TriggerEvent::Insert]);
    }

    #[test]
    fn parses_temporary_trigger() {
        let sql = "CREATE TEMPORARY TRIGGER users_audit AFTER INSERT ON users BEGIN SELECT 1; END";
        let trigger = parse_sqlite_trigger("users_audit", sql).unwrap();

        assert_eq!(trigger.name, "users_audit");
        assert_eq!(trigger.timing, TriggerTiming::After);
        assert_eq!(trigger.events, vec![TriggerEvent::Insert]);
    }

    #[test]
    fn parses_temp_trigger_if_not_exists_with_quoted_name() {
        let sql = r#"CREATE TEMP TRIGGER IF NOT EXISTS "user audit" AFTER UPDATE ON users BEGIN SELECT 1; END"#;
        let trigger = parse_sqlite_trigger("user audit", sql).unwrap();

        assert_eq!(trigger.name, "user audit");
        assert_eq!(trigger.timing, TriggerTiming::After);
        assert_eq!(trigger.events, vec![TriggerEvent::Update]);
    }
}
