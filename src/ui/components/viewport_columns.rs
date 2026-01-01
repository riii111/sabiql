const MIN_COL_WIDTH: u16 = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SlackPolicy {
    #[default]
    None,
    RightmostLimited,
}

pub struct ColumnWidthConfig<'a> {
    pub ideal_widths: &'a [u16],
    pub min_widths: &'a [u16],
}

pub struct SelectionContext {
    pub horizontal_offset: usize,
    pub available_width: u16,
    pub fixed_count: Option<usize>,
    pub max_offset: usize,
}

fn total_width_with_separators(widths: &[u16]) -> u16 {
    let sum: u16 = widths.iter().sum();
    let separators = if widths.len() > 1 {
        (widths.len() - 1) as u16
    } else {
        0
    };
    sum + separators
}

fn shrink_columns(
    widths: &mut [u16],
    min_widths: &[u16],
    indices: &[usize],
    mut excess: u16,
    from_left: bool,
) -> u16 {
    let iter: Box<dyn Iterator<Item = (usize, &mut u16)>> = if from_left {
        Box::new(widths.iter_mut().enumerate())
    } else {
        Box::new(
            widths
                .iter_mut()
                .enumerate()
                .rev()
                .collect::<Vec<_>>()
                .into_iter(),
        )
    };

    for (viewport_idx, w) in iter {
        if excess == 0 {
            break;
        }
        let col_idx = indices[viewport_idx];
        let min_w = min_widths.get(col_idx).copied().unwrap_or(MIN_COL_WIDTH);
        let shrinkable = w.saturating_sub(min_w);
        let shrink = shrinkable.min(excess);
        *w -= shrink;
        excess -= shrink;
    }

    excess
}

pub fn select_viewport_columns(
    config: &ColumnWidthConfig,
    ctx: &SelectionContext,
) -> (Vec<usize>, Vec<u16>) {
    if config.ideal_widths.is_empty() || ctx.horizontal_offset >= config.ideal_widths.len() {
        return (Vec::new(), Vec::new());
    }

    match ctx.fixed_count {
        Some(count) => select_fixed_columns(config, ctx, count),
        None => select_dynamic_columns(config, ctx.horizontal_offset, ctx.available_width),
    }
}

/// At right edge, drops leftmost column if shrinking isn't enough to preserve rightmost.
fn select_fixed_columns(
    config: &ColumnWidthConfig,
    ctx: &SelectionContext,
    count: usize,
) -> (Vec<usize>, Vec<u16>) {
    let end = (ctx.horizontal_offset + count).min(config.ideal_widths.len());
    let mut indices: Vec<usize> = (ctx.horizontal_offset..end).collect();

    if indices.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut widths: Vec<u16> = indices.iter().map(|&i| config.ideal_widths[i]).collect();
    let total_needed = total_width_with_separators(&widths);

    if total_needed > ctx.available_width {
        let excess = total_needed - ctx.available_width;
        let at_right_edge = ctx.horizontal_offset >= ctx.max_offset && ctx.max_offset > 0;

        let remaining = shrink_columns(
            &mut widths,
            config.min_widths,
            &indices,
            excess,
            at_right_edge,
        );

        if at_right_edge && remaining > 0 && indices.len() > 1 {
            indices.remove(0);
            widths.remove(0);

            if let Some(last_idx) = indices.last()
                && let Some(last_w) = widths.last_mut()
            {
                *last_w = config.ideal_widths[*last_idx];
            }

            let new_total = total_width_with_separators(&widths);
            if new_total > ctx.available_width {
                let new_excess = new_total - ctx.available_width;
                shrink_columns(&mut widths, config.min_widths, &indices, new_excess, true);
            }
        }
    }

    (indices, widths)
}

fn select_dynamic_columns(
    config: &ColumnWidthConfig,
    horizontal_offset: usize,
    available_width: u16,
) -> (Vec<usize>, Vec<u16>) {
    let mut indices = Vec::new();
    let mut widths = Vec::new();
    let mut used_width: u16 = 0;

    for (i, &width) in config
        .ideal_widths
        .iter()
        .enumerate()
        .skip(horizontal_offset)
    {
        let separator = if indices.is_empty() { 0 } else { 1 };
        let needed = width + separator;

        if used_width + needed <= available_width {
            used_width += needed;
            indices.push(i);
            widths.push(width);
        } else {
            let remaining = available_width.saturating_sub(used_width + separator);
            let min_w = config.min_widths.get(i).copied().unwrap_or(MIN_COL_WIDTH);
            if remaining >= min_w {
                indices.push(i);
                widths.push(remaining);
            }
            break;
        }
    }

    if indices.is_empty() && horizontal_offset < config.ideal_widths.len() {
        indices.push(horizontal_offset);
        widths.push(config.ideal_widths[horizontal_offset].min(available_width));
    }

    (indices, widths)
}

/// Finds max N where ALL consecutive N-column windows fit using sliding window.
/// Uses ideal_widths primarily so scrolling is enabled when content exceeds viewport,
/// even if headers (min_widths) would fit.
pub fn calculate_viewport_column_count(
    ideal_widths: &[u16],
    min_widths: &[u16],
    available_width: u16,
) -> usize {
    if ideal_widths.is_empty() {
        return 0;
    }

    for n in (1..=ideal_widths.len()).rev() {
        let all_windows_fit = (0..=ideal_widths.len() - n).all(|start| {
            let window = &ideal_widths[start..start + n];
            total_width_with_separators(window) <= available_width
        });
        if all_windows_fit {
            return n;
        }
    }

    // When ideal widths are too wide, fall back to min_widths to show something
    for n in (1..=min_widths.len()).rev() {
        let all_windows_fit = (0..=min_widths.len() - n).all(|start| {
            let window = &min_widths[start..start + n];
            total_width_with_separators(window) <= available_width
        });
        if all_windows_fit {
            return n;
        }
    }

    1
}

#[derive(Debug, Clone, Default)]
pub struct ViewportPlan {
    pub column_count: usize,
    pub max_offset: usize,
    pub available_width: u16,
    pub min_widths_sum: u16,
    pub slack_policy: SlackPolicy,
}

impl ViewportPlan {
    pub fn calculate(ideal_widths: &[u16], min_widths: &[u16], available_width: u16) -> Self {
        let column_count = calculate_viewport_column_count(ideal_widths, min_widths, available_width);
        let max_offset = calculate_max_offset(ideal_widths.len(), column_count);
        let min_widths_sum = min_widths.iter().sum();
        let slack_policy = if max_offset == 0 {
            SlackPolicy::RightmostLimited
        } else {
            SlackPolicy::None
        };

        Self {
            column_count,
            max_offset,
            available_width,
            min_widths_sum,
            slack_policy,
        }
    }

    pub fn needs_recalculation(
        &self,
        new_widths_len: usize,
        new_available_width: u16,
        new_min_widths_sum: u16,
    ) -> bool {
        self.column_count == 0
            || self.available_width != new_available_width
            || self.max_offset + self.column_count != new_widths_len
            || self.min_widths_sum != new_min_widths_sum
    }
}

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

    fn config<'a>(ideal: &'a [u16], min: &'a [u16]) -> ColumnWidthConfig<'a> {
        ColumnWidthConfig {
            ideal_widths: ideal,
            min_widths: min,
        }
    }

    fn ctx(offset: usize, width: u16, fixed: Option<usize>, max: usize) -> SelectionContext {
        SelectionContext {
            horizontal_offset: offset,
            available_width: width,
            fixed_count: fixed,
            max_offset: max,
        }
    }

    mod total_width {
        use super::*;

        #[test]
        fn empty_returns_zero() {
            assert_eq!(total_width_with_separators(&[]), 0);
        }

        #[test]
        fn single_column_no_separator() {
            assert_eq!(total_width_with_separators(&[10]), 10);
        }

        #[test]
        fn multiple_columns_includes_separators() {
            // 10 + 20 + 30 + 2 separators = 62
            assert_eq!(total_width_with_separators(&[10, 20, 30]), 62);
        }
    }

    mod column_count {
        use super::*;

        #[test]
        fn uses_ideal_widths_primarily() {
            let ideal = vec![15, 15, 15, 15];
            let min = vec![10, 10, 10, 10];
            // 3 ideal cols + 2 sep = 47 <= 50
            let count = calculate_viewport_column_count(&ideal, &min, 50);
            assert_eq!(count, 3);
        }

        #[test]
        fn scroll_enabled_when_ideal_exceeds_available() {
            let ideal = vec![30, 30, 30, 30, 30];
            let min = vec![10, 10, 10, 10, 10];
            let available = 80;

            let count = calculate_viewport_column_count(&ideal, &min, available);
            let max_offset = calculate_max_offset(5, count);

            // 30+30+1sep = 61 <= 80 → 2 columns fit
            assert_eq!(count, 2);
            assert_eq!(max_offset, 3);
        }

        #[test]
        fn falls_back_to_min_when_ideal_too_wide() {
            let ideal = vec![100, 100, 100];
            let min = vec![10, 10, 10];
            let available = 50;

            // Ideal: no fit, Min: 3 cols + 2 sep = 32 <= 50
            let count = calculate_viewport_column_count(&ideal, &min, available);
            assert_eq!(count, 3);
        }

        #[test]
        fn handles_varying_widths() {
            let ideal = vec![6, 6, 25, 6, 6];
            let min = vec![6, 6, 25, 6, 6];
            let available = 40;

            let count = calculate_viewport_column_count(&ideal, &min, available);

            // Window [1,2,3] = 6 + 25 + 6 + 2 sep = 39 <= 40 OK
            assert_eq!(count, 3);
        }

        #[test]
        fn reduces_count_when_long_column_in_middle() {
            let ideal = vec![10, 10, 50, 10, 10];
            let min = vec![10, 10, 50, 10, 10];
            let available = 60;

            let count = calculate_viewport_column_count(&ideal, &min, available);

            // Window [1,2] = 10 + 50 + 1 sep = 61 > 60, so n=2 fails
            assert_eq!(count, 1);
        }

        #[test]
        fn at_least_one_column() {
            let ideal = vec![100, 100];
            let min = vec![100, 100];
            let count = calculate_viewport_column_count(&ideal, &min, 50);
            assert_eq!(count, 1);
        }

        #[test]
        fn empty_returns_zero() {
            let count = calculate_viewport_column_count(&[], &[], 100);
            assert_eq!(count, 0);
        }
    }

    mod viewport_plan {
        use super::*;

        #[test]
        fn calculate_creates_valid_plan() {
            let ideal = vec![30, 30, 30, 30, 30];
            let min = vec![10, 10, 10, 10, 10];

            let plan = ViewportPlan::calculate(&ideal, &min, 80);

            assert_eq!(plan.column_count, 2);
            assert_eq!(plan.max_offset, 3);
            assert_eq!(plan.available_width, 80);
            assert_eq!(plan.min_widths_sum, 50);
        }

        #[test]
        fn needs_recalc_when_width_changes() {
            let plan = ViewportPlan {
                column_count: 2,
                max_offset: 3,
                available_width: 80,
                min_widths_sum: 50,
                ..Default::default()
            };

            assert!(plan.needs_recalculation(5, 100, 50));
            assert!(!plan.needs_recalculation(5, 80, 50));
        }

        #[test]
        fn needs_recalc_when_column_count_zero() {
            let plan = ViewportPlan::default();
            assert!(plan.needs_recalculation(5, 80, 50));
        }

        #[test]
        fn needs_recalc_when_widths_len_changes() {
            let plan = ViewportPlan {
                column_count: 2,
                max_offset: 3,
                available_width: 80,
                min_widths_sum: 50,
                ..Default::default()
            };

            assert!(plan.needs_recalculation(10, 80, 50));
        }
    }

    mod select_dynamic {
        use super::*;

        #[test]
        fn basic_fit() {
            let ideal = vec![10, 10, 10, 10];
            let min = vec![4, 4, 4, 4];
            let cfg = config(&ideal, &min);
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(0, 35, None, 0));
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![10, 10, 10]);
        }

        #[test]
        fn with_offset() {
            let ideal = vec![10, 10, 10, 10];
            let min = vec![4, 4, 4, 4];
            let cfg = config(&ideal, &min);
            let (indices, _) = select_viewport_columns(&cfg, &ctx(1, 25, None, 0));
            assert_eq!(indices, vec![1, 2]);
        }

        #[test]
        fn shrinks_rightmost() {
            let ideal = vec![10, 10, 50];
            let min = vec![4, 4, 4];
            let cfg = config(&ideal, &min);
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(0, 30, None, 0));
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![10, 10, 8]);
        }

        #[test]
        fn at_least_one() {
            let ideal = vec![100];
            let min = vec![4];
            let cfg = config(&ideal, &min);
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(0, 50, None, 0));
            assert_eq!(indices, vec![0]);
            assert_eq!(selected, vec![50]);
        }
    }

    mod select_fixed {
        use super::*;

        #[test]
        fn exact_count() {
            let ideal = vec![10, 10, 10, 10];
            let min = vec![4, 4, 4, 4];
            let cfg = config(&ideal, &min);
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(0, 100, Some(3), 1));
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![10, 10, 10]);
        }

        #[test]
        fn with_offset() {
            let ideal = vec![10, 10, 10, 10];
            let min = vec![4, 4, 4, 4];
            let cfg = config(&ideal, &min);
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(1, 100, Some(2), 2));
            assert_eq!(indices, vec![1, 2]);
            assert_eq!(selected, vec![10, 10]);
        }

        #[test]
        fn shrinks_to_fit_from_right() {
            let ideal = vec![20, 20, 20];
            let min = vec![4, 4, 4];
            let cfg = config(&ideal, &min);
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(0, 50, Some(3), 1));
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![20, 20, 8]);
        }

        #[test]
        fn shrinks_from_left_at_right_edge() {
            let ideal = vec![20, 20, 20];
            let min = vec![4, 4, 4];
            let cfg = config(&ideal, &min);
            // 2 cols of 20 + 1 sep = 41, available = 31, excess = 10
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(1, 31, Some(2), 1));
            assert_eq!(indices, vec![1, 2]);
            assert_eq!(selected, vec![10, 20]); // left shrinks, right preserved
        }

        #[test]
        fn shrinks_multiple_columns() {
            let ideal = vec![20, 20, 20];
            let min = vec![4, 4, 4];
            let cfg = config(&ideal, &min);
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(0, 30, Some(3), 1));
            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(selected, vec![20, 4, 4]);
        }

        #[test]
        fn respects_boundary() {
            let ideal = vec![10, 10];
            let min = vec![4, 4];
            let cfg = config(&ideal, &min);
            let (indices, _) = select_viewport_columns(&cfg, &ctx(0, 100, Some(5), 0));
            assert_eq!(indices, vec![0, 1]);
        }

        #[test]
        fn one_column_scroll_changes_one_column() {
            let ideal = vec![10, 10, 50, 10, 10];
            let min = vec![4, 4, 4, 4, 4];
            let cfg = config(&ideal, &min);
            let max_offset = 2;

            let (idx0, _) = select_viewport_columns(&cfg, &ctx(0, 75, Some(3), max_offset));
            assert_eq!(idx0, vec![0, 1, 2]);

            let (idx1, _) = select_viewport_columns(&cfg, &ctx(1, 75, Some(3), max_offset));
            assert_eq!(idx1, vec![1, 2, 3]);

            let (idx2, _) = select_viewport_columns(&cfg, &ctx(2, 75, Some(3), max_offset));
            assert_eq!(idx2, vec![2, 3, 4]);
        }
    }

    mod max_offset {
        use super::*;

        #[test]
        fn basic() {
            assert_eq!(calculate_max_offset(5, 3), 2);
        }

        #[test]
        fn all_fit() {
            assert_eq!(calculate_max_offset(3, 3), 0);
        }

        #[test]
        fn more_viewport_than_columns() {
            assert_eq!(calculate_max_offset(2, 5), 0);
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

    mod scroll_behavior {
        use super::*;

        #[test]
        fn one_scroll_changes_exactly_one_column() {
            let ideal = vec![15, 20, 30, 10, 25];
            let min = vec![8, 8, 8, 8, 8];
            let cfg = config(&ideal, &min);
            let max_offset = 2;

            let (idx0, _) = select_viewport_columns(&cfg, &ctx(0, 80, Some(3), max_offset));
            let (idx1, _) = select_viewport_columns(&cfg, &ctx(1, 80, Some(3), max_offset));
            let (idx2, _) = select_viewport_columns(&cfg, &ctx(2, 80, Some(3), max_offset));

            assert_eq!(idx0, vec![0, 1, 2]);
            assert_eq!(idx1, vec![1, 2, 3]);
            assert_eq!(idx2, vec![2, 3, 4]);
        }

        #[test]
        fn scroll_preserves_column_count() {
            let ideal = vec![10, 15, 20, 12, 18];
            let min = vec![6, 6, 6, 6, 6];
            let cfg = config(&ideal, &min);
            let max_offset = 2;

            for offset in 0..=max_offset {
                let (indices, _) =
                    select_viewport_columns(&cfg, &ctx(offset, 60, Some(3), max_offset));
                assert_eq!(
                    indices.len(),
                    3,
                    "Column count changed at offset {}",
                    offset
                );
            }
        }
    }

    mod header_min_width {
        use super::*;

        #[test]
        fn columns_never_shrink_below_header_min_width() {
            let ideal = vec![20, 20, 20];
            let min = vec![10, 10, 10];
            let cfg = config(&ideal, &min);

            let (_, selected) = select_viewport_columns(&cfg, &ctx(0, 35, Some(3), 1));

            for (i, (w, min_w)) in selected.iter().zip(min.iter()).enumerate() {
                assert!(
                    *w >= *min_w,
                    "Column {} width {} is below min {}",
                    i,
                    w,
                    min_w
                );
            }
        }

        #[test]
        fn large_min_widths_respected_under_pressure() {
            let ideal = vec![30, 30, 30];
            let min = vec![15, 15, 15];
            let cfg = config(&ideal, &min);

            let (_, selected) = select_viewport_columns(&cfg, &ctx(0, 50, Some(3), 1));

            for (w, min_w) in selected.iter().zip(min.iter()) {
                assert!(*w >= *min_w);
            }
        }

        #[test]
        fn headers_never_truncated_at_any_offset() {
            let ideal = vec![20, 20, 40, 20, 20];
            let min = vec![8, 8, 20, 8, 8];
            let available = 50;

            let count = calculate_viewport_column_count(&ideal, &min, available);
            let max_offset = calculate_max_offset(ideal.len(), count);

            for offset in 0..=max_offset {
                let cfg = config(&ideal, &min);
                let (indices, widths) =
                    select_viewport_columns(&cfg, &ctx(offset, available, Some(count), max_offset));
                for (i, &w) in widths.iter().enumerate() {
                    let col_idx = indices[i];
                    let min_w = min[col_idx];
                    assert!(
                        w >= min_w,
                        "offset={}, col={}: {} < {}",
                        offset,
                        col_idx,
                        w,
                        min_w
                    );
                }
            }
        }
    }

    mod right_edge {
        use super::*;

        #[test]
        fn rightmost_column_not_truncated_at_right_edge() {
            let ideal = vec![20, 20, 20];
            let min = vec![10, 10, 10];
            let cfg = config(&ideal, &min);
            let max_offset = 1;

            let (indices, selected) =
                select_viewport_columns(&cfg, &ctx(max_offset, 35, Some(2), max_offset));

            let last_idx = indices.len() - 1;
            assert_eq!(
                selected[last_idx], ideal[indices[last_idx]],
                "Rightmost column should have its ideal width at right edge"
            );
        }

        #[test]
        fn left_columns_shrink_first_at_right_edge() {
            let ideal = vec![20, 20, 20];
            let min = vec![10, 10, 10];
            let cfg = config(&ideal, &min);
            let max_offset = 1;

            // 2 cols of 20 + 1 sep = 41, available = 31, excess = 10
            let (_, selected) =
                select_viewport_columns(&cfg, &ctx(max_offset, 31, Some(2), max_offset));

            assert!(
                selected[0] < ideal[1],
                "Left column should be shrunk at right edge"
            );
            assert_eq!(selected[1], ideal[2], "Right column should be preserved");
        }

        #[test]
        fn drops_leftmost_when_shrinking_not_enough() {
            let ideal = vec![30, 30, 30];
            let min = vec![20, 20, 20];
            let cfg = config(&ideal, &min);
            let max_offset = 1;

            // 2 cols of 30 + 1 sep = 61, available = 35
            // After shrinking left to 20: 20 + 30 + 1 = 51 > 35, still excess
            // Should drop leftmost, keep only rightmost
            let (indices, widths) =
                select_viewport_columns(&cfg, &ctx(max_offset, 35, Some(2), max_offset));

            assert_eq!(indices.len(), 1, "Should drop to 1 column");
            assert_eq!(indices[0], 2, "Should keep rightmost column");
            assert_eq!(widths[0], 30, "Rightmost should have ideal width");
        }
    }
}
