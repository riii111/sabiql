const MIN_COL_WIDTH: u16 = 4;

/// Select columns for viewport with optional fixed column count.
/// - `fixed_count: None` → fit as many columns as possible (for initial calculation)
/// - `fixed_count: Some(n)` → always show exactly n columns, shrinking as needed
pub fn select_viewport_columns(
    all_widths: &[u16],
    horizontal_offset: usize,
    available_width: u16,
    fixed_count: Option<usize>,
    max_offset: usize,
) -> (Vec<usize>, Vec<u16>) {
    if all_widths.is_empty() || horizontal_offset >= all_widths.len() {
        return (Vec::new(), Vec::new());
    }

    match fixed_count {
        Some(count) => select_fixed_columns(
            all_widths,
            horizontal_offset,
            available_width,
            count,
            max_offset,
        ),
        None => select_dynamic_columns(all_widths, horizontal_offset, available_width),
    }
}

/// Select exactly `count` columns, shrinking from the appropriate side.
fn select_fixed_columns(
    all_widths: &[u16],
    horizontal_offset: usize,
    available_width: u16,
    count: usize,
    max_offset: usize,
) -> (Vec<usize>, Vec<u16>) {
    let end = (horizontal_offset + count).min(all_widths.len());
    let indices: Vec<usize> = (horizontal_offset..end).collect();

    if indices.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut widths: Vec<u16> = indices.iter().map(|&i| all_widths[i]).collect();

    let separators = if widths.len() > 1 {
        (widths.len() - 1) as u16
    } else {
        0
    };
    let total_needed: u16 = widths.iter().sum::<u16>() + separators;

    if total_needed > available_width {
        let mut excess = total_needed - available_width;
        let at_right_edge = horizontal_offset >= max_offset && max_offset > 0;

        if at_right_edge {
            for w in widths.iter_mut() {
                if excess == 0 {
                    break;
                }
                let shrinkable = w.saturating_sub(MIN_COL_WIDTH);
                let shrink = shrinkable.min(excess);
                *w -= shrink;
                excess -= shrink;
            }
        } else {
            for w in widths.iter_mut().rev() {
                if excess == 0 {
                    break;
                }
                let shrinkable = w.saturating_sub(MIN_COL_WIDTH);
                let shrink = shrinkable.min(excess);
                *w -= shrink;
                excess -= shrink;
            }
        }
    }

    (indices, widths)
}

/// Select as many columns as fit (dynamic mode for initial calculation).
fn select_dynamic_columns(
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
            // Try to fit with shrinking
            let remaining = available_width.saturating_sub(used_width + separator);
            if remaining >= MIN_COL_WIDTH {
                indices.push(i);
                widths.push(remaining);
            }
            break;
        }
    }

    // At least one column
    if indices.is_empty() && horizontal_offset < all_widths.len() {
        indices.push(horizontal_offset);
        widths.push(all_widths[horizontal_offset].min(available_width));
    }

    (indices, widths)
}

/// Calculate how many columns fit in the viewport (for initial setup).
pub fn calculate_viewport_column_count(all_widths: &[u16], available_width: u16) -> usize {
    let (indices, _) = select_viewport_columns(all_widths, 0, available_width, None, 0);
    indices.len()
}

/// Calculate maximum horizontal offset based on fixed column count.
pub fn calculate_max_offset(all_widths_len: usize, viewport_column_count: usize) -> usize {
    all_widths_len.saturating_sub(viewport_column_count)
}

pub fn calculate_next_column_offset(
    all_widths_len: usize,
    current_offset: usize,
    viewport_column_count: usize,
) -> usize {
    let max_offset = calculate_max_offset(all_widths_len, viewport_column_count);
    (current_offset + 1).min(max_offset)
}

pub fn calculate_prev_column_offset(current_offset: usize) -> usize {
    current_offset.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod select_dynamic {
        use super::*;

        #[test]
        fn basic_fit() {
            let widths = vec![10, 10, 10, 10];
            let (indices, selected) = select_viewport_columns(&widths, 0, 35, None, 0);
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![10, 10, 10]);
        }

        #[test]
        fn with_offset() {
            let widths = vec![10, 10, 10, 10];
            let (indices, _) = select_viewport_columns(&widths, 1, 25, None, 0);
            assert_eq!(indices, vec![1, 2]);
        }

        #[test]
        fn shrinks_rightmost() {
            let widths = vec![10, 10, 50];
            let (indices, selected) = select_viewport_columns(&widths, 0, 30, None, 0);
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![10, 10, 8]);
        }

        #[test]
        fn at_least_one() {
            let widths = vec![100];
            let (indices, selected) = select_viewport_columns(&widths, 0, 50, None, 0);
            assert_eq!(indices, vec![0]);
            assert_eq!(selected, vec![50]);
        }
    }

    mod select_fixed {
        use super::*;

        #[test]
        fn exact_count() {
            let widths = vec![10, 10, 10, 10];
            let (indices, selected) = select_viewport_columns(&widths, 0, 100, Some(3), 1);
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![10, 10, 10]);
        }

        #[test]
        fn with_offset() {
            let widths = vec![10, 10, 10, 10];
            let (indices, selected) = select_viewport_columns(&widths, 1, 100, Some(2), 2);
            assert_eq!(indices, vec![1, 2]);
            assert_eq!(selected, vec![10, 10]);
        }

        #[test]
        fn shrinks_to_fit_from_right() {
            // offset=0, max_offset=1 → not at right edge, shrink from right
            let widths = vec![20, 20, 20];
            let (indices, selected) = select_viewport_columns(&widths, 0, 50, Some(3), 1);
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![20, 20, 8]);
        }

        #[test]
        fn shrinks_from_left_at_right_edge() {
            // offset=1, max_offset=1 → at right edge, shrink from left
            // 2 cols of 20 + 1 sep = 41, available = 30, excess = 11
            let widths = vec![20, 20, 20];
            let (indices, selected) = select_viewport_columns(&widths, 1, 30, Some(2), 1);
            assert_eq!(indices, vec![1, 2]);
            assert_eq!(selected, vec![9, 20]);
        }

        #[test]
        fn shrinks_multiple_columns() {
            // offset=0, max_offset=1 → not at right edge, shrink from right
            let widths = vec![20, 20, 20];
            let (indices, selected) = select_viewport_columns(&widths, 0, 30, Some(3), 1);
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![20, 4, 4]);
        }

        #[test]
        fn respects_boundary() {
            let widths = vec![10, 10];
            let (indices, _) = select_viewport_columns(&widths, 0, 100, Some(5), 0);
            assert_eq!(indices, vec![0, 1]);
        }

        #[test]
        fn one_column_scroll_changes_one_column() {
            let widths = vec![10, 10, 50, 10, 10];
            let max_offset = 2;

            let (idx0, _) = select_viewport_columns(&widths, 0, 75, Some(3), max_offset);
            assert_eq!(idx0, vec![0, 1, 2]);

            let (idx1, _) = select_viewport_columns(&widths, 1, 75, Some(3), max_offset);
            assert_eq!(idx1, vec![1, 2, 3]);

            let (idx2, _) = select_viewport_columns(&widths, 2, 75, Some(3), max_offset);
            assert_eq!(idx2, vec![2, 3, 4]);
        }
    }

    mod max_offset {
        use super::*;

        #[test]
        fn basic() {
            // 5 columns, viewport shows 3 → max_offset = 2
            assert_eq!(calculate_max_offset(5, 3), 2);
        }

        #[test]
        fn all_fit() {
            // 3 columns, viewport shows 3 → max_offset = 0
            assert_eq!(calculate_max_offset(3, 3), 0);
        }

        #[test]
        fn more_viewport_than_columns() {
            // 2 columns, viewport shows 5 → max_offset = 0
            assert_eq!(calculate_max_offset(2, 5), 0);
        }
    }

    mod column_count {
        use super::*;

        #[test]
        fn calculates_from_dynamic() {
            let widths = vec![10, 10, 10, 10];
            let count = calculate_viewport_column_count(&widths, 35);
            assert_eq!(count, 3);
        }
    }

    mod next_prev_offset {
        use super::*;

        #[test]
        fn next_increments() {
            assert_eq!(calculate_next_column_offset(5, 1, 3), 2);
        }

        #[test]
        fn next_clamps_to_max() {
            assert_eq!(calculate_next_column_offset(5, 2, 3), 2);
        }

        #[test]
        fn prev_decrements() {
            assert_eq!(calculate_prev_column_offset(2), 1);
        }

        #[test]
        fn prev_clamps_to_zero() {
            assert_eq!(calculate_prev_column_offset(0), 0);
        }
    }
}
