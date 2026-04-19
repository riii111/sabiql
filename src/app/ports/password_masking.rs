pub(crate) fn mask_password(text: &str) -> String {
    let result = mask_url_passwords(text);
    let result = mask_kv_passwords(&result);
    mask_env_passwords(&result)
}

fn mask_url_passwords(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < text.len() {
        let scheme_len = if starts_with_ascii_ignore_case(text, i, "postgresql://") {
            "postgresql://".len()
        } else if starts_with_ascii_ignore_case(text, i, "postgres://") {
            "postgres://".len()
        } else if starts_with_ascii_ignore_case(text, i, "mysql://") {
            "mysql://".len()
        } else {
            0
        };

        if scheme_len > 0 {
            let authority_start = i + scheme_len;
            if let Some(at) = find_userinfo_terminator(text, authority_start) {
                let userinfo = text.get(authority_start..at).unwrap_or_default();
                if let Some(colon) = userinfo.find(':') {
                    let password_start = authority_start + colon + 1;
                    result.push_str(&text[i..password_start]);
                    result.push_str("****");
                    i = at;
                    continue;
                }
            }
        }

        let ch = text[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }

    result
}

fn find_userinfo_terminator(text: &str, authority_start: usize) -> Option<usize> {
    let line_end = text[authority_start..]
        .find(['\n', '\r'])
        .map_or(text.len(), |offset| authority_start + offset);
    let line = text.get(authority_start..line_end).unwrap_or_default();

    line.match_indices('@').rev().find_map(|(offset, _)| {
        let at = authority_start + offset;
        let host = text.get((at + 1)..line_end).unwrap_or_default();
        let host_end = host
            .find(['/', '?', '#', ' ', '\t', '\'', '"', ','])
            .unwrap_or(host.len());

        if host_end > 0 || host.starts_with(['/', '?', '#']) || host.is_empty() {
            Some(at)
        } else {
            None
        }
    })
}

fn mask_kv_passwords(text: &str) -> String {
    mask_after_prefix(text, |pos| password_assignment_prefix_len(text, pos))
}

fn mask_env_passwords(text: &str) -> String {
    const PREFIXES: &[&str] = &["PGPASSWORD=", "MYSQL_PASSWORD=", "MYSQL_PWD="];
    mask_after_prefix(text, |pos| {
        PREFIXES.iter().find_map(|prefix| {
            (has_assignment_boundary(text, pos) && starts_with_ascii_ignore_case(text, pos, prefix))
                .then_some(prefix.len())
        })
    })
}

fn starts_with_ascii_ignore_case(text: &str, pos: usize, needle: &str) -> bool {
    text.as_bytes()
        .get(pos..pos + needle.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(needle.as_bytes()))
}

fn has_assignment_boundary(text: &str, pos: usize) -> bool {
    pos == 0
        || text
            .as_bytes()
            .get(pos - 1)
            .is_some_and(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
}

fn password_assignment_prefix_len(text: &str, pos: usize) -> Option<usize> {
    let key = "password";
    if !has_assignment_boundary(text, pos) || !starts_with_ascii_ignore_case(text, pos, key) {
        return None;
    }

    let bytes = text.as_bytes();
    let mut i = pos + key.len();
    while bytes.get(i).is_some_and(|byte| byte.is_ascii_whitespace()) {
        i += 1;
    }
    if bytes.get(i) != Some(&b'=') {
        return None;
    }
    i += 1;
    while bytes.get(i).is_some_and(|byte| byte.is_ascii_whitespace()) {
        i += 1;
    }

    Some(i - pos)
}

fn mask_after_prefix(text: &str, find_prefix: impl Fn(usize) -> Option<usize>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < text.len() {
        if let Some(prefix_len) = find_prefix(i) {
            let eq_end = i + prefix_len;
            result.push_str(&text[i..eq_end]);
            i = skip_masked_assignment_value(text, eq_end, &mut result);
        } else {
            let ch = text[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }
    }

    result
}

fn is_assignment_terminator(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b';' | b'\'' | b'"' | b',')
}

fn skip_masked_assignment_value(text: &str, value_start: usize, result: &mut String) -> usize {
    let bytes = text.as_bytes();

    if let Some(b'\'' | b'"') = bytes.get(value_start) {
        let quote = bytes[value_start];
        result.push(quote as char);
        result.push_str("****");

        let mut i = value_start + 1;
        while i < text.len() {
            let byte = bytes[i];
            if byte == b'\\' && bytes.get(i + 1).is_some() {
                i += 2;
                continue;
            }
            if byte == quote {
                if quote == b'\'' && bytes.get(i + 1) == Some(&b'\'') {
                    i += 2;
                    continue;
                }
                result.push(quote as char);
                return i + 1;
            }
            if byte == b'\n' || byte == b'\r' {
                return i;
            }
            i += 1;
        }

        i
    } else {
        result.push_str("****");
        let mut i = value_start;
        while i < text.len() && !is_assignment_terminator(bytes[i]) {
            i += 1;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("postgres://user:secret@host", "postgres://user:****@host")]
    #[case("postgres://user:p@ss@host", "postgres://user:****@host")]
    #[case(
        "postgres://user:p@ss@host:5432/db",
        "postgres://user:****@host:5432/db"
    )]
    #[case("postgres://user:p/w@host/db", "postgres://user:****@host/db")]
    #[case(
        "postgres://user:p?w@host:5432/db",
        "postgres://user:****@host:5432/db"
    )]
    #[case("postgres://user:p#w@host", "postgres://user:****@host")]
    #[case("postgres://user:p w@host", "postgres://user:****@host")]
    #[case("postgresql://user:secret@host", "postgresql://user:****@host")]
    #[case(
        "postgresql://user:secret@/db?host=/var/run/postgresql",
        "postgresql://user:****@/db?host=/var/run/postgresql"
    )]
    #[case("mysql://user:secret@host", "mysql://user:****@host")]
    fn masks_passwords_in_urls(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[rstest]
    #[case("password=mysecret host=localhost", "password=**** host=localhost")]
    #[case("password = mysecret host=localhost", "password = **** host=localhost")]
    #[case("password= secret host=localhost", "password= **** host=localhost")]
    #[case("PGPASSWORD=secret123 psql", "PGPASSWORD=**** psql")]
    #[case("pgpassword=secret123 psql", "pgpassword=**** psql")]
    fn masks_password_assignments(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[rstest]
    #[case("password=secret;host=localhost", "password=****;host=localhost")]
    #[case("password=secret,host=localhost", "password=****,host=localhost")]
    #[case("password=secret' host=localhost", "password=****' host=localhost")]
    #[case("password=secret\" host=localhost", "password=****\" host=localhost")]
    #[case("password='secret' host=localhost", "password='****' host=localhost")]
    #[case(
        "password=\"secret\" host=localhost",
        "password=\"****\" host=localhost"
    )]
    #[case("password='se''cret' host=localhost", "password='****' host=localhost")]
    #[case(
        "password=\"sec\\\"ret\" host=localhost",
        "password=\"****\" host=localhost"
    )]
    fn stops_at_common_assignment_terminators(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[rstest]
    #[case(
        "newpassword=secret host=localhost",
        "newpassword=secret host=localhost"
    )]
    #[case(
        "old_password=secret host=localhost",
        "old_password=secret host=localhost"
    )]
    #[case("xPGPASSWORD=secret psql", "xPGPASSWORD=secret psql")]
    fn preserves_non_secret_compound_words(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[test]
    fn handles_multibyte_prefix_without_panicking() {
        assert_eq!(
            mask_password("接続先İ password=secret"),
            "接続先İ password=****"
        );
    }
}
