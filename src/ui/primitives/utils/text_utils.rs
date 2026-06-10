pub const MIN_COL_WIDTH: u16 = 4;
pub const PADDING: u16 = 2;

pub fn calculate_header_min_widths<S: AsRef<str>>(headers: &[S]) -> Vec<u16> {
    use unicode_width::UnicodeWidthStr;

    headers
        .iter()
        .map(|h| (UnicodeWidthStr::width(h.as_ref()) as u16 + PADDING).max(MIN_COL_WIDTH))
        .collect()
}

// Counts display cells, not chars: CJK text renders two cells per char and
// would otherwise overflow its column and get clipped without an ellipsis.
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }

    // The ellipsis itself must respect the width contract
    if max_width < 3 {
        return ".".repeat(max_width);
    }

    let budget = max_width - 3; // "..." occupies 3 cells
    let mut truncated = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > budget {
            break;
        }
        truncated.push(ch);
        used += w;
    }
    truncated.push_str("...");
    truncated
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
    fn calculates_min_widths_with_str_slice() {
        let headers = ["id", "name", "description"];
        let widths = calculate_header_min_widths(&headers);
        assert_eq!(widths, vec![4, 6, 13]);
    }

    #[test]
    fn calculates_min_widths_with_string_vec() {
        let headers = vec!["id".to_string(), "name".to_string()];
        let widths = calculate_header_min_widths(&headers);
        assert_eq!(widths, vec![4, 6]);
    }

    #[test]
    fn enforces_min_width() {
        let headers = ["a"];
        let widths = calculate_header_min_widths(&headers);
        assert_eq!(widths, vec![MIN_COL_WIDTH]);
    }

    #[test]
    fn cjk_headers_measured_by_display_width() {
        let headers = ["名前"];
        let widths = calculate_header_min_widths(&headers);
        // 2 chars × 2 cells + padding
        assert_eq!(widths, vec![6]);
    }

    mod truncate_to_width_tests {
        use super::super::truncate_to_width;
        use rstest::rstest;
        use unicode_width::UnicodeWidthStr;

        #[rstest]
        #[case("hello", 10, "hello")]
        #[case("hello world", 8, "hello...")]
        #[case("hello", 0, "")]
        #[case("hello", 1, ".")]
        #[case("hello", 2, "..")]
        #[case("hello", 3, "...")]
        #[case("こんにちは", 10, "こんにちは")]
        #[case("こんにちは世界", 5, "こ...")]
        #[case("日本語テスト", 10, "日本語...")]
        fn truncates_by_display_cells(
            #[case] input: &str,
            #[case] max: usize,
            #[case] expected: &str,
        ) {
            let result = truncate_to_width(input, max);

            assert_eq!(result, expected);
            assert!(UnicodeWidthStr::width(result.as_str()) <= max);
        }
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
        fn counts_wrapped_lines(#[case] text: &str, #[case] width: u16, #[case] expected: u16) {
            assert_eq!(wrapped_line_count(text, width), expected);
        }
    }
}
