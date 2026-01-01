/// Select columns for viewport, shrinking the rightmost if needed.
/// This ensures 1 scroll = 1 column change (no multi-column jumps).
pub fn select_viewport_columns(
    all_widths: &[u16],
    horizontal_offset: usize,
    available_width: u16,
) -> (Vec<usize>, Vec<u16>) {
    const MIN_COL_WIDTH: u16 = 4;

    let mut indices = Vec::new();
    let mut widths = Vec::new();
    let mut used_width: u16 = 0;

    for (i, &width) in all_widths.iter().enumerate().skip(horizontal_offset) {
        let separator = if indices.is_empty() { 0 } else { 1 };
        let needed = width + separator;

        if used_width + needed <= available_width {
            used_width += needed;
            indices.push(i);
            widths.push(width);
        } else {
            let remaining = available_width.saturating_sub(used_width + separator);
            if remaining >= MIN_COL_WIDTH {
                indices.push(i);
                widths.push(remaining);
            }
            break;
        }
    }

    if indices.is_empty() && horizontal_offset < all_widths.len() {
        indices.push(horizontal_offset);
        widths.push(all_widths[horizontal_offset].min(available_width));
    }

    (indices, widths)
}

/// Calculate maximum horizontal offset (rightmost scroll position).
pub fn calculate_max_offset(all_widths: &[u16], available_width: u16) -> usize {
    if all_widths.is_empty() {
        return 0;
    }

    let mut sum: u16 = 0;
    let mut cols_from_right = 0;

    for (i, &width) in all_widths.iter().rev().enumerate() {
        let separator = if i == 0 { 0 } else { 1 };
        let needed = width + separator;

        if sum + needed <= available_width {
            sum += needed;
            cols_from_right += 1;
        } else {
            break;
        }
    }

    let cols_from_right = cols_from_right.max(1);
    all_widths.len().saturating_sub(cols_from_right)
}

pub fn calculate_next_column_offset(
    all_widths: &[u16],
    current_offset: usize,
    available_width: u16,
) -> usize {
    let max_offset = calculate_max_offset(all_widths, available_width);
    (current_offset + 1).min(max_offset)
}

pub fn calculate_prev_column_offset(
    _all_widths: &[u16],
    current_offset: usize,
    _available_width: u16,
) -> usize {
    current_offset.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_viewport_columns_basic() {
        let widths = vec![10, 10, 10, 10];
        let (indices, selected_widths) = select_viewport_columns(&widths, 0, 35);
        assert_eq!(indices, vec![0, 1, 2]);
        assert_eq!(selected_widths, vec![10, 10, 10]);
    }

    #[test]
    fn select_viewport_columns_with_offset() {
        let widths = vec![10, 10, 10, 10];
        let (indices, _) = select_viewport_columns(&widths, 1, 25);
        assert_eq!(indices, vec![1, 2]);
    }

    #[test]
    fn select_viewport_columns_at_least_one() {
        let widths = vec![100];
        let (indices, selected_widths) = select_viewport_columns(&widths, 0, 50);
        assert_eq!(indices, vec![0]);
        assert_eq!(selected_widths, vec![50]);
    }

    #[test]
    fn calculate_max_offset_basic() {
        let widths = vec![10, 10, 10, 10];
        let max = calculate_max_offset(&widths, 25);
        assert_eq!(max, 2);
    }

    #[test]
    fn calculate_next_offset_increments_by_one() {
        let widths = vec![10, 10, 50, 10];
        let next = calculate_next_column_offset(&widths, 0, 25);
        assert_eq!(next, 1);
    }

    #[test]
    fn calculate_next_offset_clamps_to_max() {
        let widths = vec![10, 10];
        let next = calculate_next_column_offset(&widths, 0, 100);
        assert_eq!(next, 0); // all columns fit, max_offset = 0
    }

    #[test]
    fn calculate_prev_offset_decrements_by_one() {
        let widths = vec![10, 10, 10];
        let prev = calculate_prev_column_offset(&widths, 2, 25);
        assert_eq!(prev, 1);
    }

    #[test]
    fn calculate_prev_offset_clamps_to_zero() {
        let widths = vec![10, 10];
        let prev = calculate_prev_column_offset(&widths, 0, 25);
        assert_eq!(prev, 0);
    }

    #[test]
    fn select_viewport_shrinks_rightmost_column() {
        // col0(10) + sep(1) + col1(10) = 21, remaining = 30 - 21 - 1 = 8
        let widths = vec![10, 10, 50];
        let (indices, selected_widths) = select_viewport_columns(&widths, 0, 30);
        assert_eq!(indices, vec![0, 1, 2]);
        assert_eq!(selected_widths, vec![10, 10, 8]);
    }

    #[test]
    fn select_viewport_skips_if_too_narrow() {
        // col0(10) + sep(1) + col1(10) = 21, remaining = 25 - 21 - 1 = 3 < MIN(4)
        let widths = vec![10, 10, 50];
        let (indices, selected_widths) = select_viewport_columns(&widths, 0, 25);
        assert_eq!(indices, vec![0, 1]);
        assert_eq!(selected_widths, vec![10, 10]);
    }
}
