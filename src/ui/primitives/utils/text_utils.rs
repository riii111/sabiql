pub const MIN_COL_WIDTH: u16 = 4;
pub const PADDING: u16 = 2;

pub fn calculate_header_min_widths<S: AsRef<str>>(headers: &[S]) -> Vec<u16> {
    headers
        .iter()
        .map(|h| (h.as_ref().chars().count() as u16 + PADDING).max(MIN_COL_WIDTH))
        .collect()
}

pub fn wrapped_line_count(text: &str, width: u16) -> u16 {
    use unicode_width::UnicodeWidthStr;

    if width == 0 {
        return 0;
    }

    text.lines().fold(0u16, |acc, line| {
        let w = UnicodeWidthStr::width(line).min(u16::MAX as usize) as u16;
        let wrapped = w.max(1).div_ceil(width);
        acc.saturating_add(wrapped)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_header_min_widths_returns_widths_for_str_slice() {
        let headers = ["id", "name", "description"];
        let widths = calculate_header_min_widths(&headers);
        assert_eq!(widths, vec![4, 6, 13]);
    }

    #[test]
    fn calculate_header_min_widths_returns_widths_for_string_vec() {
        let headers = vec!["id".to_string(), "name".to_string()];
        let widths = calculate_header_min_widths(&headers);
        assert_eq!(widths, vec![4, 6]);
    }

    #[test]
    fn calculate_header_min_widths_enforces_min_width() {
        let headers = ["a"];
        let widths = calculate_header_min_widths(&headers);
        assert_eq!(widths, vec![MIN_COL_WIDTH]);
    }

    mod wrapped_line_count_tests {
        use super::super::wrapped_line_count;
        use rstest::rstest;

        #[rstest]
        #[case("", 80, 0)]
        #[case("hello", 80, 1)]
        #[case("hello world", 5, 3)]
        #[case("line1\nline2\nline3", 80, 3)]
        #[case("hello", 0, 0)]
        #[case("12345", 5, 1)]
        #[case("あいう", 6, 1)]
        #[case("あいう", 4, 2)]
        fn returns_expected_count(#[case] text: &str, #[case] width: u16, #[case] expected: u16) {
            assert_eq!(wrapped_line_count(text, width), expected);
        }
    }
}
