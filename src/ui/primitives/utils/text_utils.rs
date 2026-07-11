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
pub fn truncate_to_width_with(s: &str, max_width: usize, ellipsis: &str) -> String {
    use unicode_width::UnicodeWidthStr;

    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }

    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    // The ellipsis itself must respect the width contract
    if max_width < ellipsis_width {
        return take_within_width(ellipsis, max_width);
    }

    let mut truncated = take_within_width(s, max_width - ellipsis_width);
    truncated.push_str(ellipsis);
    truncated
}

pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    truncate_to_width_with(s, max_width, "...")
}

pub fn take_within_width(s: &str, budget: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    let mut taken = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > budget {
            break;
        }
        taken.push(ch);
        used += w;
    }
    taken
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

/// Wrap `text` at display-cell boundaries, preserving explicit newlines.
#[must_use]
pub fn wrap_text_lines(text: &str, width: u16) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;

    if width == 0 {
        return vec![String::new()];
    }
    let width = width as usize;

    let mut out = Vec::new();
    for line in text.split('\n') {
        let mut current = String::new();
        let mut used = 0usize;

        for ch in line.chars() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + w > width && !current.is_empty() {
                out.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push(ch);
            used += w;
        }

        out.push(current);
    }

    if out.is_empty() {
        out.push(String::new());
    }
    out
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

    mod truncate_to_width_with_tests {
        use super::super::truncate_to_width_with;
        use rstest::rstest;
        use unicode_width::UnicodeWidthStr;

        #[rstest]
        #[case("users", 16, "users")]
        #[case("exactly_16_chars", 16, "exactly_16_chars")]
        #[case("public.user_sessions", 16, "public.user_ses\u{2026}")]
        #[case("ab", 1, "\u{2026}")]
        #[case("anything", 0, "")]
        #[case("テーブル名前", 4, "テ\u{2026}")]
        fn unicode_ellipsis_respects_display_width(
            #[case] input: &str,
            #[case] max: usize,
            #[case] expected: &str,
        ) {
            let result = truncate_to_width_with(input, max, "\u{2026}");

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

    mod wrap_text_lines_tests {
        use super::super::wrap_text_lines;
        use rstest::rstest;
        use unicode_width::UnicodeWidthStr;

        #[test]
        fn short_text_single_line() {
            let lines = wrap_text_lines("hello", 10);

            assert_eq!(lines, vec!["hello".to_string()]);
        }

        #[test]
        fn wraps_greedily_by_display_width() {
            let lines = wrap_text_lines("hello world", 5);

            assert_eq!(
                lines,
                vec!["hello".to_string(), " worl".to_string(), "d".to_string()]
            );
        }

        #[test]
        fn preserves_explicit_newlines() {
            let lines = wrap_text_lines("a\nb\nc", 10);

            assert_eq!(
                lines,
                vec!["a".to_string(), "b".to_string(), "c".to_string()]
            );
        }

        #[test]
        fn empty_returns_single_empty_line() {
            let lines = wrap_text_lines("", 10);

            assert_eq!(lines, vec![String::new()]);
        }

        #[test]
        fn zero_width_returns_single_empty_line() {
            let lines = wrap_text_lines("hello", 0);

            assert_eq!(lines, vec![String::new()]);
        }

        #[rstest]
        #[case("hello world foo", 5)]
        #[case("a\nb\nc", 10)]
        #[case("こんにちは世界", 4)]
        fn every_line_within_width(#[case] text: &str, #[case] width: u16) {
            let lines = wrap_text_lines(text, width);

            for line in &lines {
                assert!(
                    UnicodeWidthStr::width(line.as_str()) <= width as usize,
                    "line {:?} exceeds width {}",
                    line,
                    width
                );
            }
        }

        #[test]
        fn cjk_wraps_by_display_cells() {
            let lines = wrap_text_lines("あいう", 4);

            // each CJK char is 2 cells: "あい" (4) | "う" (2)
            assert_eq!(lines, vec!["あい".to_string(), "う".to_string()]);
        }
    }
}
