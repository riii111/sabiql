pub const MIN_COL_WIDTH: u16 = 4;
pub const MAX_COL_WIDTH: u16 = 200;

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

    let slack = available_width - current_total;
    if let Some(rightmost) = widths.last_mut() {
        *rightmost += slack;
    }
}

fn try_add_bonus_column(
    config: &ColumnWidthConfig,
    indices: &mut Vec<usize>,
    widths: &mut Vec<u16>,
    available_width: u16,
) -> bool {
    let Some(&rightmost_idx) = indices.last() else {
        return false;
    };
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

// When the next column's ideal width doesn't fit, show it truncated as long as
// its header (min width) still fits — leaving the space blank helps no one.
fn try_add_partial_column(
    config: &ColumnWidthConfig,
    indices: &mut Vec<usize>,
    widths: &mut Vec<u16>,
    available_width: u16,
) {
    let Some(&rightmost_idx) = indices.last() else {
        return;
    };
    let next_idx = rightmost_idx + 1;

    if next_idx >= config.ideal_widths.len() {
        return;
    }

    let current_total = total_width_with_separators(widths);
    let remaining = available_width.saturating_sub(current_total + 1); // +1 for separator
    let min_w = config
        .min_widths
        .get(next_idx)
        .copied()
        .unwrap_or(MIN_COL_WIDTH);

    if remaining >= min_w {
        indices.push(next_idx);
        widths.push(remaining);
    }
}

// At the right edge no next column exists, so leftover width goes to a
// truncated peek of the previous column instead. By max_offset minimality the
// previous column never fits fully here, so a single partial prepend suffices.
fn try_prepend_partial_column(
    config: &ColumnWidthConfig,
    indices: &mut Vec<usize>,
    widths: &mut Vec<u16>,
    available_width: u16,
) {
    let Some(&leftmost_idx) = indices.first() else {
        return;
    };
    if leftmost_idx == 0 {
        return;
    }
    let prev_idx = leftmost_idx - 1;

    let current_total = total_width_with_separators(widths);
    let remaining = available_width.saturating_sub(current_total + 1); // +1 for separator
    let min_w = config
        .min_widths
        .get(prev_idx)
        .copied()
        .unwrap_or(MIN_COL_WIDTH);

    if remaining >= min_w {
        indices.insert(0, prev_idx);
        widths.insert(0, remaining.min(config.ideal_widths[prev_idx]));
    }
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

    apply_slack_to_rightmost(&mut widths, ctx.available_width);

    (indices, widths)
}

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

    if ctx.max_offset > 0 {
        while try_add_bonus_column(config, &mut indices, &mut widths, ctx.available_width) {}
        try_add_partial_column(config, &mut indices, &mut widths, ctx.available_width);
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

    if ctx.max_offset > 0 && ctx.horizontal_offset >= ctx.max_offset {
        try_prepend_partial_column(config, &mut indices, &mut widths, ctx.available_width);
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
        let separator = u16::from(!indices.is_empty());
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

// Uses ideal widths so scrolling is enabled when content exceeds the viewport.
// A single column always counts as fitting (it renders truncated); squeezing
// every column to its header width instead would disable scrolling entirely.
pub fn calculate_viewport_column_count(ideal_widths: &[u16], available_width: u16) -> usize {
    if ideal_widths.is_empty() {
        return 0;
    }

    for n in (2..=ideal_widths.len()).rev() {
        let all_windows_fit = (0..=ideal_widths.len() - n).all(|start| {
            let window = &ideal_widths[start..start + n];
            total_width_with_separators(window) <= available_width
        });
        if all_windows_fit {
            return n;
        }
    }

    1
}

// Order-sensitive: max_offset depends on suffix widths, so reordered columns
// must produce a different fingerprint even when len/sum/max are unchanged.
pub fn widths_fingerprint(ideal_widths: &[u16], min_widths: &[u16]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ideal_widths.hash(&mut hasher);
    min_widths.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, Default)]
pub struct ViewportPlan {
    pub column_count: usize,
    pub max_offset: usize,
    pub total_columns: usize,
    pub available_width: u16,
    pub widths_fingerprint: u64,
}

impl ViewportPlan {
    pub fn calculate(ideal_widths: &[u16], min_widths: &[u16], available_width: u16) -> Self {
        let column_count = calculate_viewport_column_count(ideal_widths, available_width);
        let max_offset = calculate_max_offset(ideal_widths, column_count, available_width);

        Self {
            column_count,
            max_offset,
            total_columns: ideal_widths.len(),
            available_width,
            widths_fingerprint: widths_fingerprint(ideal_widths, min_widths),
        }
    }

    pub fn needs_recalculation(&self, new_available_width: u16, new_fingerprint: u64) -> bool {
        self.column_count == 0
            || self.available_width != new_available_width
            || self.widths_fingerprint != new_fingerprint
    }

    pub fn has_horizontal_scroll(&self) -> bool {
        self.max_offset > 0
    }

    // Sized from max_offset rather than the displayed column count so the
    // indicator reaches 100% at the last scroll position
    pub fn indicator_viewport_size(&self) -> usize {
        self.total_columns.saturating_sub(self.max_offset)
    }
}

// Scrolling stops at the first offset where every remaining column fits at its
// ideal width — going further would only reveal blank space. When no suffix
// fits (an over-wide column near the end), keep the count-based bound so the
// last column stays reachable.
pub fn calculate_max_offset(
    ideal_widths: &[u16],
    viewport_column_count: usize,
    available_width: u16,
) -> usize {
    let count_based = ideal_widths.len().saturating_sub(viewport_column_count);
    (0..=count_based)
        .find(|&offset| total_width_with_separators(&ideal_widths[offset..]) <= available_width)
        .unwrap_or(count_based)
}

pub fn calculate_next_column_offset(current_offset: usize, max_offset: usize) -> usize {
    current_offset.saturating_add(1).min(max_offset)
}

pub fn calculate_prev_column_offset(current_offset: usize) -> usize {
    current_offset.saturating_sub(1)
}

#[derive(Debug, Clone, Default)]
pub struct ColumnWidthsCache {
    pub ideal_widths: Vec<u16>,
    pub header_min_widths: Vec<u16>,
    generation: u64,
    history_index: Option<usize>,
}

impl ColumnWidthsCache {
    pub fn new(
        ideal_widths: Vec<u16>,
        header_min_widths: Vec<u16>,
        generation: u64,
        history_index: Option<usize>,
    ) -> Self {
        Self {
            ideal_widths,
            header_min_widths,
            generation,
            history_index,
        }
    }

    pub fn is_valid(&self, generation: u64, history_index: Option<usize>) -> bool {
        self.generation == generation
            && self.history_index == history_index
            && !self.ideal_widths.is_empty()
    }
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
        fn empty_widths_return_zero() {
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
        fn uses_ideal_widths() {
            let ideal = vec![15, 15, 15, 15];
            // 3 ideal cols + 2 sep = 47 <= 50
            let count = calculate_viewport_column_count(&ideal, 50);
            assert_eq!(count, 3);
        }

        #[test]
        fn scroll_enabled_when_ideal_exceeds_available() {
            let ideal = vec![30, 30, 30, 30, 30];
            let available = 80;

            let count = calculate_viewport_column_count(&ideal, available);
            let max_offset = calculate_max_offset(&ideal, count, available);

            // 30+30+1sep = 61 <= 80 → 2 columns fit
            assert_eq!(count, 2);
            assert_eq!(max_offset, 3);
        }

        #[test]
        fn over_wide_columns_keep_scrolling_enabled() {
            let ideal = vec![100, 100, 100];
            let available = 50;

            // Each column exceeds the viewport: show one at a time (truncated)
            // instead of squeezing all to header width with no scrolling
            let count = calculate_viewport_column_count(&ideal, available);
            let max_offset = calculate_max_offset(&ideal, count, available);

            assert_eq!(count, 1);
            assert_eq!(max_offset, 2);
        }

        #[test]
        fn handles_varying_widths() {
            let ideal = vec![6, 6, 25, 6, 6];
            let available = 40;

            let count = calculate_viewport_column_count(&ideal, available);

            // Window [1,2,3] = 6 + 25 + 6 + 2 sep = 39 <= 40 OK
            assert_eq!(count, 3);
        }

        #[test]
        fn reduces_count_when_long_column_in_middle() {
            let ideal = vec![10, 10, 50, 10, 10];
            let available = 60;

            let count = calculate_viewport_column_count(&ideal, available);

            // Window [1,2] = 10 + 50 + 1 sep = 61 > 60, so n=2 fails
            assert_eq!(count, 1);
        }

        #[test]
        fn at_least_one_column() {
            let ideal = vec![100, 100];
            let count = calculate_viewport_column_count(&ideal, 50);
            assert_eq!(count, 1);
        }

        #[test]
        fn empty_input_returns_zero() {
            let count = calculate_viewport_column_count(&[], 100);
            assert_eq!(count, 0);
        }
    }

    mod viewport_plan {
        use super::*;

        #[test]
        fn calculate_returns_valid_plan() {
            let ideal = vec![30, 30, 30, 30, 30];
            let min = vec![10, 10, 10, 10, 10];

            let plan = ViewportPlan::calculate(&ideal, &min, 80);

            assert_eq!(plan.column_count, 2);
            assert_eq!(plan.max_offset, 3);
            assert_eq!(plan.total_columns, 5);
            assert_eq!(plan.available_width, 80);
            assert_eq!(plan.widths_fingerprint, widths_fingerprint(&ideal, &min));
        }

        #[test]
        fn same_widths_skip_recalculation() {
            let ideal = vec![10, 20, 30, 40, 50];
            let min = vec![5, 5, 5, 5, 5];
            let plan = ViewportPlan::calculate(&ideal, &min, 80);

            assert!(!plan.needs_recalculation(80, widths_fingerprint(&ideal, &min)));
        }

        #[test]
        fn recalculates_when_available_width_changes() {
            let ideal = vec![10, 20, 30];
            let min = vec![5, 5, 5];
            let plan = ViewportPlan::calculate(&ideal, &min, 80);

            assert!(plan.needs_recalculation(100, widths_fingerprint(&ideal, &min)));
        }

        #[test]
        fn recalculates_when_a_width_changes() {
            let ideal = vec![10, 20, 30, 40, 50];
            let min = vec![5, 5, 5, 5, 5];
            let plan = ViewportPlan::calculate(&ideal, &min, 80);

            let changed = vec![10, 20, 30, 40, 60];

            assert!(plan.needs_recalculation(80, widths_fingerprint(&changed, &min)));
        }

        #[test]
        fn reordered_widths_trigger_recalculation() {
            // max_offset depends on suffix widths, so a stale plan would point
            // at the wrong scroll range even though len/sum/max are unchanged
            let ideal = vec![10, 20, 30, 40, 50];
            let min = vec![5, 5, 5, 5, 5];
            let plan = ViewportPlan::calculate(&ideal, &min, 80);

            let reordered = vec![50, 40, 30, 20, 10];

            assert!(plan.needs_recalculation(80, widths_fingerprint(&reordered, &min)));
        }

        #[test]
        fn default_plan_needs_recalculation() {
            let plan = ViewportPlan::default();

            assert!(plan.needs_recalculation(80, widths_fingerprint(&[10], &[4])));
        }
    }

    mod select_dynamic {
        use super::*;

        #[test]
        fn basic_fit_absorbs_remainder_into_rightmost() {
            let ideal = vec![10, 10, 10, 10];
            let min = vec![4, 4, 4, 4];
            let cfg = config(&ideal, &min);
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(0, 35, None, 0));
            assert_eq!(indices, vec![0, 1, 2]);
            // 10+10+10 + 2 sep = 32, remainder 3 absorbed by rightmost
            assert_eq!(selected, vec![10, 10, 13]);
        }

        #[test]
        fn offset_selects_columns() {
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
            // max_offset=1, available=100, fixed=3
            // 3 cols: 32, slack=68, bonus col needs 11 → bonus added, remainder absorbed
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(0, 100, Some(3), 1));
            assert_eq!(indices, vec![0, 1, 2, 3]); // 3 fixed + 1 bonus
            assert_eq!(selected, vec![10, 10, 10, 67]);
        }

        #[test]
        fn offset_selects_fixed_columns() {
            let ideal = vec![10, 10, 10, 10];
            let min = vec![4, 4, 4, 4];
            let cfg = config(&ideal, &min);
            // max_offset=2, available=100, fixed=2
            // 2 cols: 21, slack=79, bonus col needs 11 → bonus added, remainder absorbed
            let (indices, selected) = select_viewport_columns(&cfg, &ctx(1, 100, Some(2), 2));
            assert_eq!(indices, vec![1, 2, 3]); // 2 fixed + 1 bonus
            assert_eq!(selected, vec![10, 10, 78]);
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
        fn ends_where_remaining_columns_fit() {
            let ideal = vec![10, 10, 10, 10, 10];
            // suffix from 2: 10+10+10 + 2 sep = 32 <= 32
            assert_eq!(calculate_max_offset(&ideal, 2, 32), 2);
        }

        #[test]
        fn zero_when_all_fit() {
            let ideal = vec![10, 10, 10];
            assert_eq!(calculate_max_offset(&ideal, 3, 100), 0);
        }

        #[test]
        fn more_viewport_than_columns() {
            let ideal = vec![10, 10];
            assert_eq!(calculate_max_offset(&ideal, 5, 100), 0);
        }

        #[test]
        fn stops_before_count_bound_when_tail_is_narrow() {
            let ideal = vec![30, 30, 10, 10, 10];
            // count 1 → count-based bound 4, but suffix from 2 (32) fits in 60
            assert_eq!(calculate_max_offset(&ideal, 1, 60), 2);
        }

        #[test]
        fn caps_at_count_bound_when_suffix_never_fits() {
            // Last column alone exceeds the viewport
            let ideal = vec![10, 200];
            assert_eq!(calculate_max_offset(&ideal, 1, 80), 1);
        }
    }

    mod next_prev_offset {
        use super::*;

        #[test]
        fn next_increments() {
            assert_eq!(calculate_next_column_offset(1, 2), 2);
        }

        #[test]
        fn next_clamps_to_max() {
            assert_eq!(calculate_next_column_offset(2, 2), 2);
        }

        #[test]
        fn next_saturates_at_usize_max() {
            assert_eq!(calculate_next_column_offset(usize::MAX, 2), 2);
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
        fn one_scroll_changes_first_column_by_one() {
            // With bonus columns, the total column count may vary,
            // but the FIRST column should always shift by 1 per scroll
            let ideal = vec![15, 20, 30, 10, 25];
            let min = vec![8, 8, 8, 8, 8];
            let cfg = config(&ideal, &min);
            let max_offset = 2;

            let (idx0, _) = select_viewport_columns(&cfg, &ctx(0, 80, Some(3), max_offset));
            let (idx1, _) = select_viewport_columns(&cfg, &ctx(1, 80, Some(3), max_offset));

            // First column shifts by 1 each scroll
            assert_eq!(idx0[0], 0);
            assert_eq!(idx1[0], 1);

            // At least fixed count columns are shown
            assert!(idx0.len() >= 3);
            assert!(idx1.len() >= 3);
        }

        #[test]
        fn right_edge_keeps_anchor_at_full_width_behind_prepended_peek() {
            let ideal = vec![15, 20, 30, 10, 25];
            let min = vec![8, 8, 8, 8, 8];
            let cfg = config(&ideal, &min);
            let max_offset = 2;

            let (indices, widths) = select_viewport_columns(&cfg, &ctx(2, 80, Some(3), max_offset));

            // Truncated peek of col 1 fills the leftover; anchor col 2 keeps ideal width
            assert_eq!(indices, vec![1, 2, 3, 4]);
            assert_eq!(widths, vec![12, 30, 10, 25]);
        }

        #[test]
        fn scroll_preserves_minimum_column_count() {
            // With bonus columns, count may increase but never decrease below fixed
            let ideal = vec![10, 15, 20, 12, 18];
            let min = vec![6, 6, 6, 6, 6];
            let cfg = config(&ideal, &min);
            let max_offset = 2;
            let fixed_count = 3;

            for offset in 0..=max_offset {
                let (indices, _) =
                    select_viewport_columns(&cfg, &ctx(offset, 60, Some(fixed_count), max_offset));
                assert!(
                    indices.len() >= fixed_count,
                    "Column count {} below fixed {} at offset {}",
                    indices.len(),
                    fixed_count,
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
                assert!(*w >= *min_w, "Column {i} width {w} is below min {min_w}");
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

            let count = calculate_viewport_column_count(&ideal, available);
            let max_offset = calculate_max_offset(&ideal, count, available);

            for offset in 0..=max_offset {
                let cfg = config(&ideal, &min);
                let (indices, widths) =
                    select_viewport_columns(&cfg, &ctx(offset, available, Some(count), max_offset));
                for (i, &w) in widths.iter().enumerate() {
                    let col_idx = indices[i];
                    let min_w = min[col_idx];
                    assert!(w >= min_w, "offset={offset}, col={col_idx}: {w} < {min_w}");
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
        fn fills_leftover_with_truncated_previous_column() {
            let ideal = vec![20, 8, 8];
            let min = vec![6, 6, 6];
            let cfg = config(&ideal, &min);
            let max_offset = 1;

            // Suffix [1, 2] = 17 fits in 25; col 0 (ideal 20) can't fit fully,
            // so the leftover 7 shows it truncated
            let (indices, widths) =
                select_viewport_columns(&cfg, &ctx(max_offset, 25, Some(1), max_offset));

            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(widths, vec![7, 8, 8]);
        }

        #[test]
        fn keeps_slack_in_rightmost_when_previous_header_does_not_fit() {
            let ideal = vec![20, 8, 8];
            let min = vec![15, 6, 6];
            let cfg = config(&ideal, &min);
            let max_offset = 1;

            // Leftover 7 < col 0 min 15 → absorbed by rightmost instead
            let (indices, widths) =
                select_viewport_columns(&cfg, &ctx(max_offset, 25, Some(1), max_offset));

            assert_eq!(indices, vec![1, 2]);
            assert_eq!(widths, vec![8, 16]);
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
            // 30 ideal + 5 absorbed slack
            assert_eq!(widths[0], 35, "Rightmost should fill available width");
        }
    }

    mod slack_absorption {
        use super::*;

        #[test]
        fn absorbs_slack_when_max_offset_zero() {
            let ideal = vec![10, 10, 10];
            let min = vec![4, 4, 4];
            let cfg = config(&ideal, &min);

            // 10+10+10 + 2sep = 32, available = 50, slack = 18
            let (indices, widths) = select_viewport_columns(&cfg, &ctx(0, 50, Some(3), 0));

            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(widths[0], 10);
            assert_eq!(widths[1], 10);
            assert_eq!(widths[2], 28); // 10 + 18 absorbed
        }

        #[test]
        fn absorbs_all_slack_without_limit() {
            let ideal = vec![40, 40];
            let min = vec![10, 10];
            let cfg = config(&ideal, &min);

            // 40+40 + 1sep = 81, available = 120, slack = 39
            let (_, widths) = select_viewport_columns(&cfg, &ctx(0, 120, Some(2), 0));

            assert_eq!(widths[0], 40);
            assert_eq!(widths[1], 79); // 40 + 39 (all slack absorbed)
        }

        #[test]
        fn absorbs_residual_slack_when_scrolling() {
            let ideal = vec![10, 10, 10, 20, 10];
            let min = vec![4, 4, 4, 15, 4];
            let cfg = config(&ideal, &min);

            // 3 cols: 32; col 3 gets neither bonus (needs 21 > slack 8)
            // nor partial (remaining 7 < min 15) → leftover goes to rightmost
            let (indices, widths) = select_viewport_columns(&cfg, &ctx(0, 40, Some(3), 2));

            assert_eq!(indices, vec![0, 1, 2]);
            assert_eq!(widths, vec![10, 10, 18]);
        }

        #[test]
        fn one_scroll_still_changes_one_column_with_absorption() {
            let ideal = vec![15, 15, 15, 15, 15];
            let min = vec![8, 8, 8, 8, 8];
            let cfg = config(&ideal, &min);
            let max_offset = 2;

            let (idx0, _) = select_viewport_columns(&cfg, &ctx(0, 60, Some(3), max_offset));
            let (idx1, _) = select_viewport_columns(&cfg, &ctx(1, 60, Some(3), max_offset));

            assert_eq!(idx0[0], 0);
            assert_eq!(idx1[0], 1);
        }
    }

    mod integration {
        use super::*;

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

            // Rightmost column should have at least its ideal width (40);
            // residual slack may widen it further
            let last_idx = indices.len() - 1;
            assert_eq!(
                indices[last_idx],
                ideal.len() - 1,
                "Should include the last column at right edge"
            );
            assert!(
                widths[last_idx] >= ideal[indices[last_idx]],
                "Rightmost column should not be truncated at right edge"
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
        fn column_count_at_least_fixed_during_scroll() {
            // With bonus columns, count may vary but never below fixed
            let ideal = vec![25, 30, 20, 35, 25];
            let min = vec![10, 10, 10, 10, 10];
            let available = 80;

            let plan = ViewportPlan::calculate(&ideal, &min, available);
            let fixed_count = plan.column_count;

            for offset in 0..=plan.max_offset {
                let (indices, _) = run_full_pipeline(&ideal, &min, available, offset);
                assert!(
                    indices.len() >= fixed_count,
                    "Column count {} below fixed {} at offset {}",
                    indices.len(),
                    fixed_count,
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

                // At least the guaranteed count; bonus/partial columns may add more
                assert!(indices.len() >= plan.column_count);

                // Total width should not exceed available
                let total: u16 =
                    widths.iter().sum::<u16>() + (widths.len().saturating_sub(1)) as u16;
                assert!(
                    total <= available,
                    "offset={offset}: total {total} > available {available}"
                );

                // All widths should be >= min
                for (i, &w) in widths.iter().enumerate() {
                    let col_idx = indices[i];
                    assert!(w >= min[col_idx]);
                }
            }
        }
    }

    mod bonus_column {
        use super::*;

        #[test]
        fn adds_bonus_columns_while_they_fit() {
            let ideal = vec![10, 10, 10, 10, 10];
            let min = vec![4, 4, 4, 4, 4];
            let cfg = config(&ideal, &min);

            // Fixed count = 2 (21), bonus cols 2,3,4 each need 11 → all added
            let (indices, widths) = select_viewport_columns(&cfg, &ctx(0, 70, Some(2), 3));

            assert_eq!(indices, vec![0, 1, 2, 3, 4]);
            let total = total_width_with_separators(&widths);
            assert!(total <= 70);
        }

        #[test]
        fn adds_partial_column_when_ideal_does_not_fit() {
            let ideal = vec![10, 10, 10, 20, 10];
            let min = vec![4, 4, 4, 4, 4];
            let cfg = config(&ideal, &min);

            // 3 cols: 32; col 3 needs 21 > slack 8 → no bonus,
            // but remaining 7 >= min 4 → shown truncated
            let (indices, widths) = select_viewport_columns(&cfg, &ctx(0, 40, Some(3), 2));

            assert_eq!(indices, vec![0, 1, 2, 3]);
            assert_eq!(widths, vec![10, 10, 10, 7]);
        }

        #[test]
        fn no_append_past_last_column_when_not_at_right_edge() {
            let ideal = vec![10, 10, 10];
            let min = vec![4, 4, 4];
            let cfg = config(&ideal, &min);

            // Showing cols [1, 2]; nothing exists after col 2 to append
            let (indices, _) = select_viewport_columns(&cfg, &ctx(1, 50, Some(2), 2));

            assert_eq!(indices.len(), 2);
        }

        #[test]
        fn no_bonus_when_max_offset_zero() {
            // When all columns fit (max_offset = 0), bonus logic is skipped
            let ideal = vec![10, 10, 10, 10];
            let min = vec![4, 4, 4, 4];
            let cfg = config(&ideal, &min);

            let (indices, _) = select_viewport_columns(&cfg, &ctx(0, 100, Some(3), 0));

            // Only 3 columns (fixed count), not 4
            assert_eq!(indices.len(), 3);
        }

        #[test]
        fn one_scroll_still_changes_one_column_with_bonus() {
            let ideal = vec![15, 15, 15, 15, 15, 15];
            let min = vec![8, 8, 8, 8, 8, 8];
            let cfg = config(&ideal, &min);
            // Fixed count = 3, max_offset = 3
            // With bonus, might show 4 columns

            let (idx0, _) = select_viewport_columns(&cfg, &ctx(0, 70, Some(3), 3));
            let (idx1, _) = select_viewport_columns(&cfg, &ctx(1, 70, Some(3), 3));

            // Scroll should still move by 1 (fixed count behavior)
            // idx0 starts at 0, idx1 starts at 1
            assert_eq!(idx0[0], 0, "First column at offset 0 should be 0");
            assert_eq!(idx1[0], 1, "First column at offset 1 should be 1");
        }

        #[test]
        fn total_width_within_available() {
            let ideal = vec![20, 20, 20, 15];
            let min = vec![10, 10, 10, 10];
            let cfg = config(&ideal, &min);

            // 3 cols: 20+20+20 + 2 sep = 62
            // Available: 80, slack: 18
            // Next col needs: 15 + 1 sep = 16
            // 18 >= 16, bonus added
            let (indices, widths) = select_viewport_columns(&cfg, &ctx(0, 80, Some(3), 1));

            assert_eq!(indices.len(), 4);
            let total = total_width_with_separators(&widths);
            assert!(
                total <= 80,
                "Total width {total} should not exceed available 80"
            );
        }
    }
}
