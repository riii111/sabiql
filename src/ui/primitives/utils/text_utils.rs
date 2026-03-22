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

    text.lines()
        .map(|line| {
            let w = UnicodeWidthStr::width(line) as u16;
            w.max(1).div_ceil(width)
        })
        .sum()
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

    mod wrapped_line_count_tests {
        use super::super::wrapped_line_count;

        #[test]
        fn empty_string() {
            assert_eq!(wrapped_line_count("", 80), 0);
        }

        #[test]
        fn single_line_shorter_than_width() {
            assert_eq!(wrapped_line_count("hello", 80), 1);
        }

        #[test]
        fn single_line_longer_than_width() {
            assert_eq!(wrapped_line_count("hello world", 5), 3);
        }

        #[test]
        fn multiline() {
            assert_eq!(wrapped_line_count("line1\nline2\nline3", 80), 3);
        }

        #[test]
        fn zero_width() {
            assert_eq!(wrapped_line_count("hello", 0), 0);
        }

        #[test]
        fn exact_width() {
            assert_eq!(wrapped_line_count("12345", 5), 1);
        }

        #[test]
        fn cjk_double_width() {
            // "あいう" = 3 chars but display width 6
            assert_eq!(wrapped_line_count("あいう", 6), 1);
            assert_eq!(wrapped_line_count("あいう", 4), 2);
        }
    }
}
