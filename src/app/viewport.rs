pub const MIN_COL_WIDTH: u16 = 4;
pub const MAX_COL_WIDTH: u16 = 50;

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
    pub slack_policy: SlackPolicy,
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
    let len = widths.len();
    if len == 0 {
        return excess;
    }

    if from_left {
        for viewport_idx in 0..len {
            if excess == 0 {
                break;
            }
            let col_idx = indices[viewport_idx];
            let min_w = min_widths.get(col_idx).copied().unwrap_or(MIN_COL_WIDTH);
            let shrinkable = widths[viewport_idx].saturating_sub(min_w);
            let shrink = shrinkable.min(excess);
            widths[viewport_idx] -= shrink;
            excess -= shrink;
        }
    } else {
        for viewport_idx in (0..len).rev() {
            if excess == 0 {
                break;
            }
            let col_idx = indices[viewport_idx];
            let min_w = min_widths.get(col_idx).copied().unwrap_or(MIN_COL_WIDTH);
            let shrinkable = widths[viewport_idx].saturating_sub(min_w);
            let shrink = shrinkable.min(excess);
            widths[viewport_idx] -= shrink;
            excess -= shrink;
        }
    }

    excess
}

fn apply_slack_to_rightmost(widths: &mut [u16], available_width: u16) {
    if widths.is_empty() {
        return;
    }

    let current_total = total_width_with_separators(widths);
    if current_total >= available_width {
        return;
    }

    // No upper limit: only called when max_offset == 0 (no scroll needed)
    let slack = available_width - current_total;
    if let Some(rightmost) = widths.last_mut() {
        *rightmost += slack;
    }
}

/// Attempts to add a bonus column if slack space allows.
/// Returns true if bonus column was added.
fn try_add_bonus_column(
    config: &ColumnWidthConfig,
    indices: &mut Vec<usize>,
    widths: &mut Vec<u16>,
    available_width: u16,
) -> bool {
    if indices.is_empty() {
        return false;
    }

    let rightmost_idx = indices.last().copied().unwrap_or(0);
    let next_idx = rightmost_idx + 1;

    if next_idx >= config.ideal_widths.len() {
        return false;
    }

    let current_total = total_width_with_separators(widths);
    let slack = available_width.saturating_sub(current_total);

    let next_ideal_width = config.ideal_widths[next_idx];
    let needed = next_ideal_width + 1; // +1 for separator

    if slack >= needed {
        indices.push(next_idx);
        widths.push(next_ideal_width);
        return true;
    }

    false
}

pub fn select_viewport_columns(
    config: &ColumnWidthConfig,
    ctx: &SelectionContext,
) -> (Vec<usize>, Vec<u16>) {
    if config.ideal_widths.is_empty() || ctx.horizontal_offset >= config.ideal_widths.len() {
        return (Vec::new(), Vec::new());
    }

    let (indices, mut widths) = match ctx.fixed_count {
        Some(count) => select_fixed_columns(config, ctx, count),
        None => select_dynamic_columns(config, ctx.horizontal_offset, ctx.available_width),
    };

    if ctx.slack_policy == SlackPolicy::RightmostLimited {
        apply_slack_to_rightmost(&mut widths, ctx.available_width);
    }

    (indices, widths)
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

    // Add bonus column when scrolling is needed (max_offset > 0)
    if ctx.max_offset > 0 {
        try_add_bonus_column(config, &mut indices, &mut widths, ctx.available_width);
    }

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
    pub ideal_widths_sum: u16,
    pub ideal_widths_max: u16,
    pub slack_policy: SlackPolicy,
}

impl ViewportPlan {
    pub fn calculate(ideal_widths: &[u16], min_widths: &[u16], available_width: u16) -> Self {
        let column_count =
            calculate_viewport_column_count(ideal_widths, min_widths, available_width);
        let max_offset = calculate_max_offset(ideal_widths.len(), column_count);
        let min_widths_sum = min_widths.iter().sum();
        let ideal_widths_sum = ideal_widths.iter().sum();
        let ideal_widths_max = ideal_widths.iter().copied().max().unwrap_or(0);
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
            ideal_widths_sum,
            ideal_widths_max,
            slack_policy,
        }
    }

    pub fn needs_recalculation(
        &self,
        new_widths_len: usize,
        new_available_width: u16,
        new_min_widths_sum: u16,
        new_ideal_widths_sum: u16,
        new_ideal_widths_max: u16,
    ) -> bool {
        self.column_count == 0
            || self.available_width != new_available_width
            || self.max_offset + self.column_count != new_widths_len
            || self.min_widths_sum != new_min_widths_sum
            || self.ideal_widths_sum != new_ideal_widths_sum
            || self.ideal_widths_max != new_ideal_widths_max
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
            slack_policy: SlackPolicy::None,
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
        use rstest::rstest;

        #[test]
        fn calculate_returns_valid_plan() {
            let ideal = vec![30, 30, 30, 30, 30];
            let min = vec![10, 10, 10, 10, 10];

            let plan = ViewportPlan::calculate(&ideal, &min, 80);

            assert_eq!(plan.column_count, 2);
            assert_eq!(plan.max_offset, 3);
            assert_eq!(plan.available_width, 80);
            assert_eq!(plan.min_widths_sum, 50);
            assert_eq!(plan.ideal_widths_sum, 150);
            assert_eq!(plan.ideal_widths_max, 30);
        }

        fn make_plan() -> ViewportPlan {
            ViewportPlan {
                column_count: 2,
                max_offset: 3,
                available_width: 80,
                min_widths_sum: 50,
                ideal_widths_sum: 150,
                ideal_widths_max: 30,
                ..Default::default()
            }
        }

        #[rstest]
        #[case(5, 100, 50, 150, 30, true)] // width changes
        #[case(5, 80, 50, 150, 30, false)] // no change
        #[case(10, 80, 50, 150, 30, true)] // widths_len changes
        #[case(5, 80, 60, 150, 30, true)] // min_widths_sum changes
        #[case(5, 80, 50, 200, 30, true)] // ideal_widths_sum changes
        #[case(5, 80, 50, 150, 50, true)] // ideal_widths_max changes
        fn needs_recalculation_returns_expected(
            #[case] len: usize,
            #[case] width: u16,
            #[case] min_sum: u16,
            #[case] ideal_sum: u16,
            #[case] ideal_max: u16,
            #[case] expected: bool,
        ) {
            let plan = make_plan();

            let result = plan.needs_recalculation(len, width, min_sum, ideal_sum, ideal_max);

            assert_eq!(result, expected);
        }

        #[test]
        fn default_plan_needs_recalculation() {
            let plan = ViewportPlan::default();

            let result = plan.needs_recalculation(5, 80, 50, 150, 30);

            assert!(result);
        }

        #[test]
        fn different_max_triggers_recalculation() {
            // Scenario: Data in a column changed width
            // Original: [10, 20, 30, 40, 50] sum=150, max=50
            // Changed:  [10, 20, 30, 40, 60] sum=160, max=60
            let original_ideal = vec![10, 20, 30, 40, 50];
            let original_min = vec![5, 5, 5, 5, 5];
            let plan = ViewportPlan::calculate(&original_ideal, &original_min, 80);

            let changed = [10, 20, 30, 40, 60];
            let sum: u16 = changed.iter().sum();
            let max: u16 = changed.iter().copied().max().unwrap();

            let result = plan.needs_recalculation(5, 80, 25, sum, max);

            assert!(result);
        }

        #[test]
        fn same_sum_and_max_skips_recalculation() {
            // Edge case: reordering doesn't change sum or max
            // Original: [10, 20, 30, 40, 50] sum=150, max=50
            // Changed:  [50, 40, 30, 20, 10] sum=150, max=50
            let original_ideal = vec![10, 20, 30, 40, 50];
            let original_min = vec![5, 5, 5, 5, 5];
            let plan = ViewportPlan::calculate(&original_ideal, &original_min, 80);

            let reordered = [50, 40, 30, 20, 10];
            let sum: u16 = reordered.iter().sum();
            let max: u16 = reordered.iter().copied().max().unwrap();

            let result = plan.needs_recalculation(5, 80, 25, sum, max);

            // Known limitation: reordering with same sum+max is not detected
            assert!(!result);
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

    mod slack_absorption {
        use super::*;

        fn ctx_with_slack(
            offset: usize,
            width: u16,
            fixed: Option<usize>,
            max: usize,
            policy: SlackPolicy,
        ) -> SelectionContext {
            SelectionContext {
                horizontal_offset: offset,
                available_width: width,
                fixed_count: fixed,
                max_offset: max,
                slack_policy: policy,
            }
        }

        #[test]
        fn absorbs_slack_when_max_offset_zero() {
            let ideal = vec![10, 10, 10];
            let min = vec![4, 4, 4];
            let cfg = config(&ideal, &min);

            // 10+10+10 + 2sep = 32, available = 50, slack = 18
            let (indices, widths) = select_viewport_columns(
                &cfg,
                &ctx_with_slack(0, 50, Some(3), 0, SlackPolicy::RightmostLimited),
            );

            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(widths[0], 10);
            assert_eq!(widths[1], 10);
            assert_eq!(widths[2], 28); // 10 + 18 absorbed
        }

        #[test]
        fn no_absorption_when_policy_none() {
            let ideal = vec![10, 10, 10];
            let min = vec![4, 4, 4];
            let cfg = config(&ideal, &min);

            let (_, widths) = select_viewport_columns(
                &cfg,
                &ctx_with_slack(0, 50, Some(3), 0, SlackPolicy::None),
            );

            assert_eq!(widths, vec![10, 10, 10]);
        }

        #[test]
        fn absorbs_all_slack_without_limit() {
            let ideal = vec![40, 40];
            let min = vec![10, 10];
            let cfg = config(&ideal, &min);

            // 40+40 + 1sep = 81, available = 120, slack = 39
            let (_, widths) = select_viewport_columns(
                &cfg,
                &ctx_with_slack(0, 120, Some(2), 0, SlackPolicy::RightmostLimited),
            );

            assert_eq!(widths[0], 40);
            assert_eq!(widths[1], 79); // 40 + 39 (all slack absorbed)
        }

        #[test]
        fn viewport_plan_sets_policy_based_on_max_offset() {
            let ideal = vec![10, 10, 10];
            let min = vec![4, 4, 4];

            // All columns fit → max_offset = 0 → RightmostLimited
            let plan = ViewportPlan::calculate(&ideal, &min, 100);
            assert_eq!(plan.max_offset, 0);
            assert_eq!(plan.slack_policy, SlackPolicy::RightmostLimited);

            // Need scroll → max_offset > 0 → None
            let plan2 = ViewportPlan::calculate(&ideal, &min, 25);
            assert!(plan2.max_offset > 0);
            assert_eq!(plan2.slack_policy, SlackPolicy::None);
        }

        #[test]
        fn one_scroll_still_changes_one_column_with_slack_none() {
            let ideal = vec![15, 15, 15, 15, 15];
            let min = vec![8, 8, 8, 8, 8];
            let cfg = config(&ideal, &min);
            let max_offset = 2;

            let (idx0, _) = select_viewport_columns(
                &cfg,
                &ctx_with_slack(0, 60, Some(3), max_offset, SlackPolicy::None),
            );
            let (idx1, _) = select_viewport_columns(
                &cfg,
                &ctx_with_slack(1, 60, Some(3), max_offset, SlackPolicy::None),
            );
            let (idx2, _) = select_viewport_columns(
                &cfg,
                &ctx_with_slack(2, 60, Some(3), max_offset, SlackPolicy::None),
            );

            assert_eq!(idx0, vec![0, 1, 2]);
            assert_eq!(idx1, vec![1, 2, 3]);
            assert_eq!(idx2, vec![2, 3, 4]);
        }
    }

    /// Integration tests that simulate the full workflow as used in Result/Inspector.
    /// These tests verify that the plan → selection → width calculation pipeline
    /// produces correct results, particularly at edge cases.
    mod integration {
        use super::*;

        /// Simulates the workflow in Result/Inspector:
        /// 1. Calculate ViewportPlan from ideal/min widths
        /// 2. Use plan to create SelectionContext
        /// 3. Select viewport columns
        /// 4. Verify widths are correct
        fn run_full_pipeline(
            ideal_widths: &[u16],
            min_widths: &[u16],
            available_width: u16,
            horizontal_offset: usize,
        ) -> (Vec<usize>, Vec<u16>) {
            let plan = ViewportPlan::calculate(ideal_widths, min_widths, available_width);
            let clamped_offset = horizontal_offset.min(plan.max_offset);

            let cfg = ColumnWidthConfig {
                ideal_widths,
                min_widths,
            };
            let ctx = SelectionContext {
                horizontal_offset: clamped_offset,
                available_width,
                fixed_count: Some(plan.column_count),
                max_offset: plan.max_offset,
                slack_policy: plan.slack_policy,
            };

            select_viewport_columns(&cfg, &ctx)
        }

        #[test]
        fn right_edge_preserves_rightmost_column_width() {
            let ideal = vec![20, 25, 30, 15, 40];
            let min = vec![10, 10, 10, 10, 10];
            let available = 70;

            let plan = ViewportPlan::calculate(&ideal, &min, available);
            let max_offset = plan.max_offset;

            // At right edge (max offset)
            let (indices, widths) = run_full_pipeline(&ideal, &min, available, max_offset);

            // Rightmost column should have its ideal width (40)
            let last_idx = indices.len() - 1;
            assert_eq!(
                indices[last_idx],
                ideal.len() - 1,
                "Should include the last column at right edge"
            );
            assert_eq!(
                widths[last_idx], ideal[indices[last_idx]],
                "Rightmost column should have ideal width at right edge"
            );
        }

        #[test]
        fn headers_never_truncated_throughout_scroll() {
            let ideal = vec![15, 20, 35, 18, 22];
            let min = vec![8, 10, 15, 8, 10];
            let available = 60;

            let plan = ViewportPlan::calculate(&ideal, &min, available);

            for offset in 0..=plan.max_offset {
                let (indices, widths) = run_full_pipeline(&ideal, &min, available, offset);

                for (i, &w) in widths.iter().enumerate() {
                    let col_idx = indices[i];
                    assert!(
                        w >= min[col_idx],
                        "offset={}, col={}: width {} < min {}",
                        offset,
                        col_idx,
                        w,
                        min[col_idx]
                    );
                }
            }
        }

        #[test]
        fn column_count_stable_during_scroll() {
            let ideal = vec![25, 30, 20, 35, 25];
            let min = vec![10, 10, 10, 10, 10];
            let available = 80;

            let plan = ViewportPlan::calculate(&ideal, &min, available);
            let expected_count = plan.column_count;

            for offset in 0..=plan.max_offset {
                let (indices, _) = run_full_pipeline(&ideal, &min, available, offset);
                assert_eq!(
                    indices.len(),
                    expected_count,
                    "Column count changed at offset {}",
                    offset
                );
            }
        }

        #[test]
        fn slack_absorbed_when_no_scroll_needed() {
            let ideal = vec![15, 15, 15];
            let min = vec![8, 8, 8];
            let available = 80;

            let plan = ViewportPlan::calculate(&ideal, &min, available);
            assert_eq!(plan.max_offset, 0, "Should fit all columns");
            assert_eq!(plan.slack_policy, SlackPolicy::RightmostLimited);

            let (_, widths) = run_full_pipeline(&ideal, &min, available, 0);

            // Total: 15+15+15 + 2sep = 47, available = 80, slack = 33
            // Rightmost should absorb slack
            let total: u16 = widths.iter().sum::<u16>() + (widths.len() as u16 - 1);
            assert_eq!(total, available, "Should use all available width");
        }

        #[test]
        fn realistic_result_pane_scenario() {
            // Simulate a table with columns: id, name, email, created_at, status
            let ideal = vec![6, 20, 35, 22, 10]; // Realistic column widths
            let min = vec![4, 6, 8, 12, 8]; // Header widths
            let available = 80;

            let plan = ViewportPlan::calculate(&ideal, &min, available);

            // Verify plan is calculated correctly
            assert!(plan.column_count > 0);
            assert!(plan.column_count <= ideal.len());

            // Test scrolling through all positions
            for offset in 0..=plan.max_offset {
                let (indices, widths) = run_full_pipeline(&ideal, &min, available, offset);

                // Column count should be stable
                assert_eq!(indices.len(), plan.column_count);

                // Total width should not exceed available
                let total: u16 =
                    widths.iter().sum::<u16>() + (widths.len().saturating_sub(1)) as u16;
                assert!(
                    total <= available,
                    "offset={}: total {} > available {}",
                    offset,
                    total,
                    available
                );

                // All widths should be >= min
                for (i, &w) in widths.iter().enumerate() {
                    let col_idx = indices[i];
                    assert!(w >= min[col_idx]);
                }
            }
        }
    }
}
