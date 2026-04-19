pub(crate) fn mask_password(text: &str) -> String {
    let result = mask_url_passwords(text);
    let result = mask_kv_passwords(&result);
    mask_env_passwords(&result)
}

fn mask_url_passwords(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < text.len() {
        let remaining = &lower[i..];
        let scheme_len = if remaining.starts_with("postgresql://") {
            "postgresql://".len()
        } else if remaining.starts_with("postgres://") {
            "postgres://".len()
        } else if remaining.starts_with("mysql://") {
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
    let lower = text.to_lowercase();
    mask_after_prefix(text, |pos| {
        let needle = "password=";
        lower[pos..].starts_with(needle).then_some(needle.len())
    })
}

fn mask_env_passwords(text: &str) -> String {
    const PREFIXES: &[&str] = &["PGPASSWORD=", "MYSQL_PASSWORD=", "MYSQL_PWD="];
    mask_after_prefix(text, |pos| {
        PREFIXES.iter().find_map(|prefix| {
            text[pos..]
                .get(..prefix.len())
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
                .then_some(prefix.len())
        })
    })
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
}
