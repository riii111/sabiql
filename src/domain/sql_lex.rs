pub fn skip_line_comment(chars: &[(usize, char)], i: usize, ch: char) -> Option<usize> {
    if ch != '-' || !next_char_is(chars, i, '-') {
        return None;
    }
    let mut cursor = i;
    while cursor < chars.len() && chars[cursor].1 != '\n' {
        cursor += 1;
    }
    Some(cursor)
}

pub fn skip_block_comment(chars: &[(usize, char)], i: usize, ch: char) -> Option<usize> {
    if ch != '/' || !next_char_is(chars, i, '*') {
        return None;
    }
    let mut cursor = i + 2;
    while cursor + 1 < chars.len() && !(chars[cursor].1 == '*' && chars[cursor + 1].1 == '/') {
        cursor += 1;
    }
    Some(cursor + 2)
}

pub fn advance_single_quote(
    chars: &[(usize, char)],
    i: usize,
    ch: char,
    in_string: &mut bool,
) -> Option<usize> {
    if ch != '\'' {
        return None;
    }
    if *in_string {
        if next_char_is(chars, i, '\'') {
            return Some(i + 2);
        }
        *in_string = false;
    } else {
        *in_string = true;
    }
    Some(i + 1)
}

pub fn skip_double_quoted_identifier(chars: &[(usize, char)], i: usize, ch: char) -> Option<usize> {
    if ch != '"' {
        return None;
    }
    let mut cursor = i + 1;
    while cursor < chars.len() {
        if chars[cursor].1 == '"' {
            if next_char_is(chars, cursor, '"') {
                cursor += 2;
            } else {
                cursor += 1;
                break;
            }
        } else {
            cursor += 1;
        }
    }
    Some(cursor)
}

pub fn skip_sqlite_quoted_identifier(chars: &[(usize, char)], i: usize, ch: char) -> Option<usize> {
    let close = match ch {
        '`' => '`',
        '[' => ']',
        _ => return None,
    };
    let mut cursor = i + 1;
    while cursor < chars.len() {
        if chars[cursor].1 == close {
            if close == '`' && next_char_is(chars, cursor, close) {
                cursor += 2;
            } else {
                cursor += 1;
                break;
            }
        } else {
            cursor += 1;
        }
    }
    Some(cursor)
}

fn next_char_is(chars: &[(usize, char)], i: usize, expected: char) -> bool {
    i + 1 < chars.len() && chars[i + 1].1 == expected
}
