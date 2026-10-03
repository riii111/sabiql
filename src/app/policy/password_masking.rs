pub fn mask_password(text: &str) -> String {
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
                if let Some(colon) = userinfo.find(':')
                    && userinfo
                        .get(colon + 1..)
                        .is_some_and(|password| !password.is_empty())
                {
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

    mask_uri_query_passwords(&result)
}

fn mask_uri_query_passwords(text: &str) -> String {
    let mut ranges = Vec::new();
    for (query_start, token_end) in uri_query_ranges(text) {
        let query = &text[query_start..token_end];
        let mut segment_start = query_start;
        for segment in query.split('&') {
            let segment_end = segment_start + segment.len();
            if let Some(equal_offset) = segment.find('=') {
                let key = &segment[..equal_offset];
                if urlencoding::decode(key).is_ok_and(|key| {
                    key.eq_ignore_ascii_case("password") || key.eq_ignore_ascii_case("sslpassword")
                }) {
                    ranges.push((segment_start + equal_offset + 1, segment_end));
                }
            }
            segment_start = segment_end.saturating_add(1);
        }
    }

    let mut result = text.to_string();
    for (start, end) in ranges.into_iter().rev() {
        result.replace_range(start..end, "****");
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
    let query_ranges = uri_query_ranges(text);
    let mut query_index = 0;
    mask_after_prefix(text, |pos| {
        let prefix_len = password_assignment_prefix_len(text, pos)?;
        while query_ranges
            .get(query_index)
            .is_some_and(|(_, end)| pos >= *end)
        {
            query_index += 1;
        }
        let in_query = query_ranges
            .get(query_index)
            .is_some_and(|(start, _)| pos >= *start);
        (!in_query).then_some(prefix_len)
    })
}

fn uri_query_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let scheme_len = ["postgresql://", "postgres://", "mysql://"]
            .into_iter()
            .find(|scheme| starts_with_ascii_ignore_case(text, i, scheme))
            .map_or(0, str::len);
        if scheme_len == 0 {
            i += text[i..].chars().next().unwrap().len_utf8();
            continue;
        }
        let authority_start = i + scheme_len;
        let token_end = uri_token_end(text, authority_start);
        if let Some(query_offset) = text[authority_start..token_end].find('?') {
            ranges.push((authority_start + query_offset + 1, token_end));
        }
        i = token_end;
    }
    ranges
}

fn uri_token_end(text: &str, start: usize) -> usize {
    let mut query_started = false;
    let mut key_start = None;
    let mut sensitive_value = false;
    for (offset, ch) in text[start..].char_indices() {
        let pos = start + offset;
        match ch {
            '\n' | '\r' | ' ' | '\t' | '\'' | '"' => return pos,
            '?' if !query_started => {
                query_started = true;
                key_start = Some(pos + 1);
            }
            '&' if query_started => {
                key_start = Some(pos + 1);
                sensitive_value = false;
            }
            '=' if key_start.is_some() => {
                let key = &text[key_start.take().unwrap()..pos];
                sensitive_value = urlencoding::decode(key).is_ok_and(|key| {
                    key.eq_ignore_ascii_case("password") || key.eq_ignore_ascii_case("sslpassword")
                });
            }
            ',' if !sensitive_value => return pos,
            _ => {}
        }
    }
    text.len()
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
    const KEYS: &[&str] = &["password", "sslpassword"];

    let bytes = text.as_bytes();
    if let Some(key) = KEYS.iter().find(|key| {
        has_assignment_boundary(text, pos) && starts_with_ascii_ignore_case(text, pos, key)
    }) {
        let mut i = pos + key.len();
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            return None;
        }
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }

        return Some(i - pos);
    }

    if !has_assignment_boundary(text, pos) {
        return None;
    }
    let mut key_end = pos;
    // Each byte of "sslpassword" can be represented by at most three percent-encoded bytes.
    const MAX_ENCODED_KEY_BYTES: usize = "sslpassword".len() * 3;
    while key_end - pos <= MAX_ENCODED_KEY_BYTES
        && bytes
            .get(key_end)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'=')
    {
        key_end += 1;
    }
    if key_end - pos > MAX_ENCODED_KEY_BYTES {
        return None;
    }
    let encoded_key = text.get(pos..key_end)?;
    let decoded_key = urlencoding::decode(encoded_key).ok()?;
    if !decoded_key.eq_ignore_ascii_case("password")
        && !decoded_key.eq_ignore_ascii_case("sslpassword")
    {
        return None;
    }
    if bytes.get(key_end) != Some(&b'=') {
        return None;
    }
    let mut i = key_end + 1;
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }

    Some(i - pos)
}

fn mask_after_prefix(text: &str, mut find_prefix: impl FnMut(usize) -> Option<usize>) -> String {
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
    #[case("mysql://user:@host", "mysql://user:@host")]
    #[case(
        "mysql://user:p@ss%23word@host:3306/db?ssl-mode=REQUIRED",
        "mysql://user:****@host:3306/db?ssl-mode=REQUIRED"
    )]
    #[case(
        "postgresql://user@host/db?pass%77ord=secret&sslmode=require",
        "postgresql://user@host/db?pass%77ord=****&sslmode=require"
    )]
    #[case(
        "postgresql://user@host/db?password=ab#cd&sslmode=require",
        "postgresql://user@host/db?password=****&sslmode=require"
    )]
    fn masks_passwords_in_urls(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[rstest]
    #[case("password=mysecret host=localhost", "password=**** host=localhost")]
    #[case("password = mysecret host=localhost", "password = **** host=localhost")]
    #[case("password= secret host=localhost", "password= **** host=localhost")]
    #[case(
        "sslpassword=mysecret host=localhost",
        "sslpassword=**** host=localhost"
    )]
    #[case(
        "sslpassword = mysecret host=localhost",
        "sslpassword = **** host=localhost"
    )]
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
    #[case("password=ab#cd", "password=****")]
    #[case("password=ab&cd", "password=****")]
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
    fn ignores_non_password_boundaries(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[rstest]
    #[case(
        "postgresql://host/db?password=abc,def&sslmode=require",
        "postgresql://host/db?password=****&sslmode=require"
    )]
    #[case(
        "postgres://host/db?sslpassword=abc,def&password=ab?cd",
        "postgres://host/db?sslpassword=****&password=****"
    )]
    #[case("MYSQL://host/db?password=abc,def", "MYSQL://host/db?password=****")]
    #[case(
        "postgresql://host/db?password=abc,def password=outside",
        "postgresql://host/db?password=**** password=****"
    )]
    fn masks_entire_query_password_with_commas(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[test]
    fn masks_comma_separated_uris_without_splitting_password_values() {
        assert_eq!(
            mask_password(
                "postgresql://one/db?application_name=x,postgresql://two/db?password=secret"
            ),
            "postgresql://one/db?application_name=x,postgresql://two/db?password=****"
        );
        assert_eq!(
            mask_password("postgresql://one/db?application_name=x,password=secret"),
            "postgresql://one/db?application_name=x,password=****"
        );
        assert_eq!(
            mask_password("%73%73%6c%70%61%73%73%77%6f%72%64=secret"),
            "%73%73%6c%70%61%73%73%77%6f%72%64=****"
        );
        assert_eq!(
            mask_password("postgresql://one/db?password=abc,postgresql://suffix"),
            "postgresql://one/db?password=****"
        );
        assert_eq!(
            mask_password(&format!("{} password=secret", "-".repeat(256_000))),
            format!("{} password=****", "-".repeat(256_000))
        );
    }

    #[test]
    fn preserves_long_non_uri_text_and_masks_following_assignments() {
        let plain = "x".repeat(256_000);
        let input = format!(
            "{plain} password=secret postgresql://host/db?password=abc,def password=outside"
        );

        assert_eq!(
            mask_password(&input),
            format!("{plain} password=**** postgresql://host/db?password=**** password=****")
        );
    }

    #[test]
    fn handles_multibyte_input_without_boundary_mismatch() {
        assert_eq!(
            mask_password("接続先İ password=secret"),
            "接続先İ password=****"
        );
    }
}
