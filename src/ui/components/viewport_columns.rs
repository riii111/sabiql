/// Select columns that fit within available width starting from offset.
/// Returns (column_indices, column_widths).
pub fn select_viewport_columns(
    all_widths: &[u16],
    horizontal_offset: usize,
    available_width: u16,
) -> (Vec<usize>, Vec<u16>) {
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
            break;
        }
    }

    // Always show at least one column
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

/// Calculate offset to reveal the next hidden column (scroll right).
/// Ensures one keypress always reveals a new column.
pub fn calculate_next_column_offset(
    all_widths: &[u16],
    current_offset: usize,
    available_width: u16,
) -> usize {
    let (visible_indices, _) = select_viewport_columns(all_widths, current_offset, available_width);

    let last_visible = visible_indices.last().copied().unwrap_or(current_offset);
    let next_hidden = last_visible + 1;

    if next_hidden >= all_widths.len() {
        return current_offset;
    }

    for new_offset in (current_offset + 1)..=next_hidden {
        let (new_visible, _) = select_viewport_columns(all_widths, new_offset, available_width);
        if new_visible.contains(&next_hidden) {
            return new_offset;
        }
    }

    next_hidden.min(all_widths.len().saturating_sub(1))
}

/// Calculate offset to reveal the previous hidden column (scroll left).
/// Ensures one keypress always reveals a new column.
pub fn calculate_prev_column_offset(
    all_widths: &[u16],
    current_offset: usize,
    available_width: u16,
) -> usize {
    if current_offset == 0 {
        return 0;
    }

    let prev_hidden = current_offset - 1;

    for new_offset in (0..current_offset).rev() {
        let (new_visible, _) = select_viewport_columns(all_widths, new_offset, available_width);
        if new_visible.contains(&prev_hidden) {
            return new_offset;
        }
    }

    0
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
    fn calculate_next_column_offset_reveals_next() {
        let widths = vec![10, 10, 50, 10];
        let next = calculate_next_column_offset(&widths, 0, 25);
        assert_eq!(next, 2);
    }

    #[test]
    fn calculate_next_column_offset_at_end() {
        let widths = vec![10, 10];
        let next = calculate_next_column_offset(&widths, 0, 100);
        assert_eq!(next, 0);
    }

    #[test]
    fn calculate_prev_column_offset_reveals_prev() {
        let widths = vec![10, 10, 10];
        let prev = calculate_prev_column_offset(&widths, 2, 25);
        assert_eq!(prev, 1);
    }

    #[test]
    fn calculate_prev_column_offset_at_start() {
        let widths = vec![10, 10];
        let prev = calculate_prev_column_offset(&widths, 0, 25);
        assert_eq!(prev, 0);
    }
}
