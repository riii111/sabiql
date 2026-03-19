pub(super) fn cursor_to_position(content: &str, cursor_pos: usize) -> (usize, usize) {
    let mut row = 0;
    let mut col = 0;

    for (current_pos, ch) in content.chars().enumerate() {
        if current_pos >= cursor_pos {
            break;
        }
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (row, col)
}

#[cfg(test)]
pub fn position_to_cursor(content: &str, row: usize, col: usize) -> usize {
    let mut current_row = 0;
    let mut current_col = 0;
    let mut cursor_pos = 0;

    for ch in content.chars() {
        if current_row == row && current_col == col {
            return cursor_pos;
        }
        if ch == '\n' {
            if current_row == row {
                return cursor_pos;
            }
            current_row += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
        cursor_pos += 1;
    }

    cursor_pos
}

#[cfg(test)]
pub fn line_lengths(content: &str) -> Vec<usize> {
    content.lines().map(|l| l.chars().count()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn empty_string_returns_zero_position() {
        let result = cursor_to_position("", 0);

        assert_eq!(result, (0, 0));
    }

    #[test]
    fn single_line_ascii_returns_correct_position() {
        let content = "SELECT * FROM users";

        let result = cursor_to_position(content, 7);

        assert_eq!(result, (0, 7));
    }

    #[test]
    fn multiple_lines_returns_correct_row_and_col() {
        let content = "SELECT *\nFROM users\nWHERE id = 1";

        let result = cursor_to_position(content, 17);

        assert_eq!(result, (1, 8));
    }

    #[test]
    fn multibyte_characters_count_correctly() {
        let content = "こんにちは世界";

        let result = cursor_to_position(content, 3);

        assert_eq!(result, (0, 3));
    }

    #[rstest]
    #[case("SELECT 日本語", 7, (0, 7))]
    #[case("SELECT 日本語", 8, (0, 8))]
    #[case("SELECT 日本語", 9, (0, 9))]
    #[case("こんにちは\n世界", 5, (0, 5))]
    #[case("こんにちは\n世界", 6, (1, 0))]
    #[case("こんにちは\n世界", 7, (1, 1))]
    fn multibyte_cursor_positions_are_accurate(
        #[case] input: &str,
        #[case] cursor: usize,
        #[case] expected: (usize, usize),
    ) {
        let result = cursor_to_position(input, cursor);

        assert_eq!(result, expected);
    }

    #[test]
    fn position_to_cursor_converts_back_correctly() {
        let content = "SELECT *\nFROM users";

        let cursor = position_to_cursor(content, 1, 5);

        assert_eq!(cursor, 14);
    }

    #[test]
    fn position_to_cursor_with_multibyte_returns_correct_index() {
        let content = "こんにちは\n世界";

        let cursor = position_to_cursor(content, 1, 2);

        assert_eq!(cursor, 8);
    }

    #[test]
    fn cursor_at_end_of_line_returns_line_length() {
        let content = "SELECT *\nFROM users";

        let cursor = position_to_cursor(content, 0, 100);

        assert_eq!(cursor, 8);
    }

    #[test]
    fn line_lengths_counts_chars_not_bytes() {
        let content = "abc\n日本語\nxyz";

        let lengths = line_lengths(content);

        assert_eq!(lengths, vec![3, 3, 3]);
    }

    #[rstest]
    #[case("", vec![])]
    #[case("single", vec![6])]
    #[case("one\ntwo", vec![3, 3])]
    #[case("あ\nい\nう", vec![1, 1, 1])]
    fn line_lengths_handles_various_inputs(#[case] input: &str, #[case] expected: Vec<usize>) {
        let result = line_lengths(input);

        assert_eq!(result, expected);
    }
}
