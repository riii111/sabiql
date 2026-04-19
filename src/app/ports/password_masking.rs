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
            let authority_end = text[authority_start..]
                .find(['/', '?', '#', ' ', '\n', '\r', '\t'])
                .map_or(text.len(), |offset| authority_start + offset);
            let authority = &text[authority_start..authority_end];

            if let Some(at) = authority.rfind('@') {
                let userinfo = &authority[..at];
                if let Some(colon) = userinfo.find(':') {
                    let password_start = authority_start + colon + 1;
                    let password_end = authority_start + at;
                    result.push_str(&text[i..password_start]);
                    result.push_str("****");
                    result.push_str(&text[password_end..authority_end]);
                    i = authority_end;
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

fn mask_kv_passwords(text: &str) -> String {
    mask_after_prefix(text, |pos| {
        let needle = "password=";
        (has_assignment_boundary(text, pos) && starts_with_ascii_ignore_case(text, pos, needle))
            .then_some(needle.len())
    })
}

fn mask_env_passwords(text: &str) -> String {
    const PREFIXES: &[&str] = &["PGPASSWORD=", "MYSQL_PASSWORD=", "MYSQL_PWD="];
    mask_after_prefix(text, |pos| {
        PREFIXES.iter().find_map(|prefix| {
            (has_assignment_boundary(text, pos)
                && starts_with_ascii_ignore_case(text, pos, prefix))
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

fn mask_after_prefix(text: &str, find_prefix: impl Fn(usize) -> Option<usize>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < text.len() {
        if let Some(prefix_len) = find_prefix(i) {
            let eq_end = i + prefix_len;
            result.push_str(&text[i..eq_end]);
            result.push_str("****");
            let mut j = eq_end;
            while j < text.len() && !text.as_bytes()[j].is_ascii_whitespace() {
                j += 1;
            }
            i = j;
        } else {
            let ch = text[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("postgres://user:secret@host", "postgres://user:****@host")]
    #[case("postgres://user:p@ss@host", "postgres://user:****@host")]
    #[case("postgres://user:p@ss@host:5432/db", "postgres://user:****@host:5432/db")]
    #[case("postgresql://user:secret@host", "postgresql://user:****@host")]
    #[case("mysql://user:secret@host", "mysql://user:****@host")]
    fn masks_passwords_in_urls(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[rstest]
    #[case("password=mysecret host=localhost", "password=**** host=localhost")]
    #[case("PGPASSWORD=secret123 psql", "PGPASSWORD=**** psql")]
    #[case("pgpassword=secret123 psql", "pgpassword=**** psql")]
    fn masks_password_assignments(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(mask_password(input), expected);
    }

    #[rstest]
    #[case("newpassword=secret host=localhost", "newpassword=secret host=localhost")]
    #[case("old_password=secret host=localhost", "old_password=secret host=localhost")]
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
