//! Wrapped Cell Mode layout engine.
//!
//! When Wrapped Cell Mode is active and horizontal scrolling is disabled, the
//! entire result set must fit inside the pane width without a horizontal
//! scrollbar. Columns are shrunk to fit and cell text is wrapped, expanding
//! rows vertically so the whole cell content is visible.
//!
//! This module is pure: it takes widths/text/available space and returns the
//! computed layout. Rendering lives in the UI layer; this is the geometry.

use std::hash::{Hash, Hasher};

use unicode_width::UnicodeWidthStr;

fn wrapped_line_count(text: &str, width: u16) -> u16 {
    if width == 0 || text.is_empty() {
        return 0;
    }

    text.split('\n').fold(0u16, |acc, line| {
        let w = UnicodeWidthStr::width(line).min(u16::MAX as usize) as u16;
        let wrapped = w.max(1).div_ceil(width);
        acc.saturating_add(wrapped)
    })
}

/// Wrapped Cell Mode settings: persisted config stored in the settings UI.
/// Default disables horizontal scroll (columns shrink to fit) with no row cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WrappedCellSettings {
    pub allow_horizontal_scroll: bool,
    pub max_lines_per_row: Option<u16>,
}

impl WrappedCellSettings {
    #[must_use]
    pub fn effective_wrap_width(col_width: u16, padding: u16) -> u16 {
        col_width.saturating_sub(padding).max(1)
    }

    #[must_use]
    pub fn wrapped_cell_lines(text: &str, col_width: u16, padding: u16) -> u16 {
        let wrap_width = Self::effective_wrap_width(col_width, padding);
        wrapped_line_count(text, wrap_width)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedCellColumn {
    pub width: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedCellRow {
    pub height: u16,
    pub truncated: bool,
}

/// Complete Wrapped Cell Mode layout: one width per column, one height per row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WrappedCellLayout {
    pub columns: Vec<WrappedCellColumn>,
    pub rows: Vec<WrappedCellRow>,
}

/// Identifies the inputs a measured layout was computed for, so the renderer
/// can reuse a cached layout across frames instead of re-measuring every row.
///
/// The per-row heights only change when one of these changes: a new result
/// (`result_generation`), a pane resize (`inner_width`, which drives the
/// shrunk column widths), a Wrapped Cell setting toggle, or the visible
/// horizontal viewport. During plain vertical scrolling none of these move,
/// so the cached vector is reused and the O(rows) measurement is skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WrappedCellLayoutKey {
    pub result_generation: u64,
    pub inner_width: u16,
    pub allow_horizontal_scroll: bool,
    pub max_lines_per_row: Option<u16>,
    pub viewport_fingerprint: u64,
}

impl WrappedCellLayoutKey {
    #[must_use]
    pub fn with_viewport(mut self, indices: &[usize], widths: &[u16]) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        indices.hash(&mut hasher);
        widths.hash(&mut hasher);
        self.viewport_fingerprint = hasher.finish();
        self
    }
}

/// Per-row line heights measured by the renderer for line-based scroll math.
///
/// `line_prefix` is the prefix sum of clamped row heights, enabling O(1)/O(log n)
/// visibility lookups instead of O(n) row walks — critical for large result sets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MeasuredWrappedCellLayout {
    pub row_heights: Vec<u16>,
    pub line_prefix: Vec<usize>,
    pub key: WrappedCellLayoutKey,
}

impl MeasuredWrappedCellLayout {
    #[must_use]
    pub fn new(row_heights: Vec<u16>, key: WrappedCellLayoutKey) -> Self {
        let mut line_prefix = Vec::with_capacity(row_heights.len() + 1);
        let mut acc = 0usize;
        line_prefix.push(0);
        for &h in &row_heights {
            acc = acc.saturating_add(h.max(1) as usize);
            line_prefix.push(acc);
        }
        Self {
            row_heights,
            line_prefix,
            key,
        }
    }

    #[must_use]
    pub fn total_lines(&self) -> usize {
        self.line_prefix.last().copied().unwrap_or(0)
    }

    #[must_use]
    pub fn row_height(&self, row: usize) -> usize {
        self.row_heights.get(row).map_or(1, |&h| h.max(1) as usize)
    }

    fn lines_before(&self, row: usize) -> usize {
        let idx = row.min(self.row_heights.len());
        self.line_prefix[idx]
    }

    fn row_offset_at_or_before(&self, line: usize) -> usize {
        self.line_prefix
            .partition_point(|&start| start <= line)
            .saturating_sub(1)
    }

    /// Clamp `scroll_offset` so `target_row` stays visible, using prefix sums
    /// for O(log n) visibility math instead of O(n) row walks.
    #[must_use]
    pub fn ensure_row_visible(
        &self,
        scroll_offset: usize,
        target_row: usize,
        line_budget: usize,
    ) -> usize {
        if line_budget == 0 || self.row_heights.is_empty() {
            return scroll_offset;
        }

        let last = self.row_heights.len() - 1;
        let target = target_row.min(last);

        if target < scroll_offset {
            return target;
        }

        let lines_above = self.lines_before(target) - self.lines_before(scroll_offset);
        let target_height = self.row_height(target);
        if lines_above + target_height <= line_budget {
            return scroll_offset;
        }

        let span_end = self.line_prefix[target + 1];
        let threshold = span_end.saturating_sub(line_budget);
        self.line_prefix
            .partition_point(|&lines| lines < threshold)
            .min(target)
    }

    /// Maximum row scroll offset: largest offset where trailing content still
    /// fills the pane. O(log n) via binary search on prefix sums.
    #[must_use]
    pub fn max_row_offset(&self, line_budget: usize) -> usize {
        let n = self.row_heights.len();
        if line_budget == 0 || n == 0 {
            return 0;
        }
        let total = self.total_lines();
        if total <= line_budget {
            return 0;
        }
        let threshold = total - line_budget;
        self.line_prefix
            .partition_point(|&lines| lines < threshold)
            .min(n - 1)
    }

    #[must_use]
    pub fn scroll_offset_for_center(&self, target_row: usize, line_budget: usize) -> usize {
        if line_budget == 0 || self.row_heights.is_empty() {
            return 0;
        }
        let target = target_row.min(self.row_heights.len() - 1);
        let desired_start = self.lines_before(target).saturating_sub(line_budget / 2);
        let candidate = self.row_offset_at_or_before(desired_start);
        self.ensure_row_visible(candidate, target, line_budget)
            .min(target)
            .min(self.max_row_offset(line_budget))
    }

    #[must_use]
    pub fn scroll_offset_for_top(&self, target_row: usize, line_budget: usize) -> usize {
        if line_budget == 0 || self.row_heights.is_empty() {
            return 0;
        }
        target_row
            .min(self.row_heights.len() - 1)
            .min(self.max_row_offset(line_budget))
    }

    #[must_use]
    pub fn scroll_offset_for_bottom(&self, target_row: usize, line_budget: usize) -> usize {
        if line_budget == 0 || self.row_heights.is_empty() {
            return 0;
        }
        let target = target_row.min(self.row_heights.len() - 1);
        let desired_start = self.lines_before(target + 1).saturating_sub(line_budget);
        let candidate = self.row_offset_at_or_before(desired_start);
        self.ensure_row_visible(candidate, target, line_budget)
            .min(target)
            .min(self.max_row_offset(line_budget))
    }
}

impl WrappedCellLayout {
    #[must_use]
    pub fn total_width(&self) -> u16 {
        total_width_with_separators(&self.column_widths())
    }

    fn column_widths(&self) -> Vec<u16> {
        self.columns.iter().map(|c| c.width).collect()
    }
}

fn total_width_with_separators(widths: &[u16]) -> u16 {
    let sum: u32 = widths.iter().map(|&w| u32::from(w)).sum();
    let separators = if widths.len() > 1 {
        (widths.len() - 1) as u32
    } else {
        0
    };
    (sum + separators).min(u16::MAX as u32) as u16
}

/// Compute per-column widths (shrunk to fit) and per-row heights (from
/// wrapped cell text) for Wrapped Cell Mode.
#[must_use]
pub fn compute_layout(
    headers: &[String],
    rows: &[Vec<String>],
    ideal_widths: &[u16],
    available_width: u16,
    settings: &WrappedCellSettings,
    padding: u16,
) -> WrappedCellLayout {
    if ideal_widths.is_empty() {
        return WrappedCellLayout::default();
    }

    let column_widths = shrink_columns_to_fit(ideal_widths, available_width);
    let columns: Vec<WrappedCellColumn> = column_widths
        .iter()
        .map(|&w| WrappedCellColumn { width: w })
        .collect();

    let row_cap = settings.max_lines_per_row;

    let computed_rows: Vec<WrappedCellRow> = rows
        .iter()
        .map(|row| row_layout(row, &column_widths, padding, row_cap))
        .collect();

    // `headers` is kept in the signature for API compatibility but is not used
    // here because column widths are derived purely from data dimensions.
    let _ = headers;

    WrappedCellLayout {
        columns,
        rows: computed_rows,
    }
}

/// Shrink ideal widths to fit `available_width`, preserving their ratios.
///
/// Columns normally have a minimum width of 4. If one width per column and
/// its separators cannot fit, returns an empty layout for horizontal scrolling.
#[must_use]
pub fn shrink_columns_to_fit(ideal_widths: &[u16], available_width: u16) -> Vec<u16> {
    const MIN_COL_WIDTH: u16 = 4;

    if ideal_widths.is_empty() {
        return ideal_widths.to_vec();
    }

    let minimum_total = ideal_widths.len().saturating_mul(2).saturating_sub(1);
    if usize::from(available_width) < minimum_total {
        return Vec::new();
    }

    let total = total_width_with_separators(ideal_widths);
    if total <= available_width {
        return ideal_widths.to_vec();
    }

    let separator_overhead = ideal_widths.len().saturating_sub(1) as u16;
    let content_budget = available_width.saturating_sub(separator_overhead);

    distribute_budget(ideal_widths, content_budget, MIN_COL_WIDTH)
}

fn distribute_budget(ideal: &[u16], budget: u16, min_width: u16) -> Vec<u16> {
    let n = ideal.len();
    if n == 0 {
        return Vec::new();
    }

    let min_total = min_width.saturating_mul(n as u16);
    if budget <= min_total {
        // Budget too tight for even minimum widths — scale down proportionally
        // so we still fit within the pane.
        let min_sum: u32 = min_width as u32 * n as u32;
        if budget == 0 {
            let per_col = budget.saturating_div(n.max(1) as u16);
            return vec![per_col.max(1); n];
        }
        let mut scaled: Vec<u16> = ideal
            .iter()
            .map(|&w| {
                let s = (u32::from(w) * u32::from(budget))
                    .checked_div(min_sum)
                    .unwrap_or_else(|| u32::from(min_width)) as u16;
                s.max(1)
            })
            .collect();
        reconcile_to_budget(&mut scaled, budget, 1);
        return scaled;
    }

    let ideal_total: u32 = ideal.iter().map(|&w| u32::from(w)).sum();
    if ideal_total == 0 {
        return vec![min_width; n];
    }

    let mut widths: Vec<u16> = ideal
        .iter()
        .map(|&w| {
            let scaled = (u32::from(w) * u32::from(budget) / ideal_total) as u16;
            scaled.max(min_width)
        })
        .collect();

    // Integer rounding + min-width floor can shift totals; reconcile.
    reconcile_to_budget(&mut widths, budget, min_width);

    widths
}

fn reconcile_to_budget(widths: &mut [u16], budget: u16, min_width: u16) {
    let target = budget;

    loop {
        let current: u32 = widths.iter().map(|&w| u32::from(w)).sum();
        if current == u32::from(target) {
            break;
        }

        if current > u32::from(target) {
            let Some(widest) = widest_shrinkable_index(widths, min_width) else {
                break;
            };
            widths[widest] -= 1;
        } else {
            let Some(narrowest) = narrowest_index(widths) else {
                break;
            };
            widths[narrowest] += 1;
        }
    }
}

fn widest_shrinkable_index(widths: &[u16], min_width: u16) -> Option<usize> {
    widths
        .iter()
        .enumerate()
        .filter(|(_, w)| **w > min_width)
        .max_by_key(|(_, w)| **w)
        .map(|(i, _)| i)
}

fn narrowest_index(widths: &[u16]) -> Option<usize> {
    widths
        .iter()
        .enumerate()
        .min_by_key(|(_, w)| **w)
        .map(|(i, _)| i)
}

pub fn row_layout(
    row: &[String],
    column_widths: &[u16],
    padding: u16,
    row_cap: Option<u16>,
) -> WrappedCellRow {
    if let Some(cap) = row_cap {
        let overflowed = overflowed_any(row, column_widths, padding, cap);
        let raw_max = max_wrapped_lines(row, column_widths, padding);
        return WrappedCellRow {
            height: raw_max.min(cap).max(1),
            truncated: overflowed,
        };
    }

    WrappedCellRow {
        height: max_wrapped_lines(row, column_widths, padding).max(1),
        truncated: false,
    }
}

fn max_wrapped_lines(row: &[String], column_widths: &[u16], padding: u16) -> u16 {
    row.iter()
        .enumerate()
        .map(|(col_idx, cell)| {
            column_widths.get(col_idx).map_or(1, |&col_width| {
                WrappedCellSettings::wrapped_cell_lines(cell, col_width, padding)
            })
        })
        .max()
        .unwrap_or(1)
}

fn overflowed_any(row: &[String], column_widths: &[u16], padding: u16, cap: u16) -> bool {
    row.iter().enumerate().any(|(col_idx, cell)| {
        column_widths.get(col_idx).is_some_and(|&col_width| {
            WrappedCellSettings::wrapped_cell_lines(cell, col_width, padding) > cap
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod settings {
        use super::*;

        #[test]
        fn default_disables_scroll_and_no_cap() {
            let s = WrappedCellSettings::default();

            assert!(!s.allow_horizontal_scroll);
            assert_eq!(s.max_lines_per_row, None);
        }

        #[rstest]
        #[case(10, 2, 8)]
        #[case(2, 2, 1)]
        #[case(1, 2, 1)]
        #[case(0, 2, 1)]
        fn wrap_width_subtracts_padding(#[case] col: u16, #[case] pad: u16, #[case] expected: u16) {
            assert_eq!(
                WrappedCellSettings::effective_wrap_width(col, pad),
                expected
            );
        }

        #[rstest]
        #[case("hello", 10, 2, 1)]
        #[case("hello world foo", 8, 2, 3)]
        #[case("a\nb\nc", 10, 2, 3)]
        #[case("", 10, 2, 0)]
        fn wrapped_cell_lines_counts_display(
            #[case] text: &str,
            #[case] col: u16,
            #[case] pad: u16,
            #[case] expected: u16,
        ) {
            assert_eq!(
                WrappedCellSettings::wrapped_cell_lines(text, col, pad),
                expected
            );
        }
    }

    mod shrink_columns_to_fit {
        use super::*;

        #[test]
        fn unchanged_when_fits() {
            let widths = vec![10, 10, 10];

            let result = shrink_columns_to_fit(&widths, 100);

            assert_eq!(result, vec![10, 10, 10]);
        }

        #[test]
        fn shrinks_proportionally() {
            let widths = vec![40, 10];

            let result = shrink_columns_to_fit(&widths, 21);

            assert_eq!(result.len(), 2);
            assert!(result[0] > result[1]);
            let total = result[0] + result[1] + 1;
            assert_eq!(total, 21);
        }

        #[test]
        fn scales_below_min_width_when_budget_is_too_tight() {
            let widths = vec![100, 100, 100, 100];

            let result = shrink_columns_to_fit(&widths, 10);

            assert!(result.iter().all(|&width| width >= 1));
            let total = result.iter().sum::<u16>() + (result.len() - 1) as u16;
            assert_eq!(total, 10);
        }

        #[test]
        fn returns_empty_when_separators_cannot_fit() {
            let result = shrink_columns_to_fit(&[100, 100, 100, 100], 6);

            assert!(result.is_empty());
            let total = result.iter().sum::<u16>() + result.len().saturating_sub(1) as u16;
            assert!(total <= 6);
        }

        #[test]
        fn single_column_clamps_to_available() {
            let widths = vec![100];

            let result = shrink_columns_to_fit(&widths, 30);

            assert_eq!(result, vec![30]);
        }

        #[test]
        fn empty_returns_empty() {
            let result = shrink_columns_to_fit(&[], 30);

            assert!(result.is_empty());
        }

        #[test]
        fn total_with_separators_matches_available() {
            let widths = vec![15, 25, 10, 30];

            let result = shrink_columns_to_fit(&widths, 40);

            let total = result.iter().sum::<u16>() + (result.len() - 1) as u16;
            assert_eq!(total, 40);
        }

        #[test]
        fn equal_columns_get_equal_share() {
            let widths = vec![20, 20, 20];

            let result = shrink_columns_to_fit(&widths, 30);

            assert_eq!(result, vec![10, 9, 9]);
            let total = result.iter().sum::<u16>() + 2;
            assert_eq!(total, 30);
        }
    }

    mod compute_layout {
        use super::*;

        fn settings(allow_scroll: bool, cap: Option<u16>) -> WrappedCellSettings {
            WrappedCellSettings {
                allow_horizontal_scroll: allow_scroll,
                max_lines_per_row: cap,
            }
        }

        #[test]
        fn empty_columns_returns_empty_layout() {
            let layout = compute_layout(&[], &[], &[], 40, &settings(false, None), 2);

            assert!(layout.columns.is_empty());
            assert!(layout.rows.is_empty());
        }

        #[test]
        fn columns_fit_available_width() {
            let headers = vec!["a".to_string(), "b".to_string()];
            let rows = vec![vec!["1".to_string(), "2".to_string()]];
            let ideal = vec![40, 40];

            let layout = compute_layout(&headers, &rows, &ideal, 20, &settings(false, None), 2);

            assert_eq!(layout.total_width(), 20);
        }

        #[test]
        fn row_height_is_max_wrapped_lines_across_cells() {
            let headers = vec!["a".to_string(), "b".to_string()];
            let rows = vec![vec![
                "short".to_string(),
                "this is a longer value".to_string(),
            ]];
            let ideal = vec![10, 10];

            let layout = compute_layout(&headers, &rows, &ideal, 21, &settings(false, None), 2);

            assert_eq!(layout.rows.len(), 1);
            assert!(layout.rows[0].height >= 2);
            assert!(!layout.rows[0].truncated);
        }

        #[test]
        fn different_rows_get_different_heights() {
            let headers = vec!["a".to_string()];
            let rows = vec![
                vec!["short".to_string()],
                vec!["a much longer value that wraps".to_string()],
            ];
            let ideal = vec![20];

            let layout = compute_layout(&headers, &rows, &ideal, 20, &settings(false, None), 2);

            assert!(layout.rows[0].height < layout.rows[1].height);
        }

        #[test]
        fn row_cap_clamps_height_and_marks_truncated() {
            let headers = vec!["a".to_string()];
            let long_text = "word ".repeat(20);
            let rows = vec![vec![long_text]];
            let ideal = vec![12];

            let layout = compute_layout(&headers, &rows, &ideal, 12, &settings(false, Some(3)), 2);

            assert_eq!(layout.rows[0].height, 3);
            assert!(layout.rows[0].truncated);
        }

        #[test]
        fn row_cap_not_marked_truncated_when_content_fits() {
            let headers = vec!["a".to_string()];
            let rows = vec![vec!["short".to_string()]];
            let ideal = vec![12];

            let layout = compute_layout(&headers, &rows, &ideal, 12, &settings(false, Some(10)), 2);

            assert_eq!(layout.rows[0].height, 1);
            assert!(!layout.rows[0].truncated);
        }

        #[test]
        fn empty_cell_row_has_min_height_one() {
            let headers = vec!["a".to_string()];
            let rows = vec![vec![String::new()]];
            let ideal = vec![12];

            let layout = compute_layout(&headers, &rows, &ideal, 12, &settings(false, None), 2);

            assert_eq!(layout.rows[0].height, 1);
        }
    }

    mod total_width {
        use super::*;

        #[test]
        fn includes_separators() {
            let layout = WrappedCellLayout {
                columns: vec![
                    WrappedCellColumn { width: 10 },
                    WrappedCellColumn { width: 20 },
                ],
                rows: vec![],
            };

            assert_eq!(layout.total_width(), 31);
        }

        #[test]
        fn single_column_no_separator() {
            let layout = WrappedCellLayout {
                columns: vec![WrappedCellColumn { width: 15 }],
                rows: vec![],
            };

            assert_eq!(layout.total_width(), 15);
        }
    }

    mod line_based_visibility {
        use super::*;

        fn layout(heights: Vec<u16>) -> MeasuredWrappedCellLayout {
            MeasuredWrappedCellLayout::new(heights, WrappedCellLayoutKey::default())
        }

        #[test]
        fn row_height_defaults_to_one_out_of_range() {
            let l = layout(vec![3, 5]);

            assert_eq!(l.row_height(10), 1);
            assert_eq!(l.row_height(0), 3);
            assert_eq!(l.row_height(1), 5);
        }

        #[test]
        fn total_lines_sums_clamped_heights() {
            assert_eq!(layout(vec![3, 1, 5]).total_lines(), 9);
            assert_eq!(layout(vec![0, 0]).total_lines(), 2);
            assert_eq!(layout(vec![]).total_lines(), 0);
        }

        #[test]
        fn ensure_visible_returns_unchanged_when_already_visible() {
            let l = layout(vec![1, 1, 1, 1, 1]);

            assert_eq!(l.ensure_row_visible(0, 2, 5), 0);
        }

        #[test]
        fn ensure_visible_advances_minimally_when_below() {
            let l = layout(vec![1, 1, 1, 1, 1, 1]);

            assert_eq!(l.ensure_row_visible(0, 4, 3), 2);
        }

        #[test]
        fn ensure_visible_accounts_for_tall_rows() {
            let l = layout(vec![5, 1]);

            assert_eq!(l.ensure_row_visible(0, 1, 4), 1);
        }

        #[test]
        fn ensure_visible_pins_target_to_top_when_too_tall() {
            let l = layout(vec![10]);

            assert_eq!(l.ensure_row_visible(0, 0, 4), 0);
        }

        #[test]
        fn ensure_visible_retreats_when_target_above() {
            let l = layout(vec![1, 1, 1, 1, 1]);

            assert_eq!(l.ensure_row_visible(4, 1, 3), 1);
        }

        #[test]
        fn ensure_visible_handles_empty_heights() {
            let l = layout(vec![]);

            assert_eq!(l.ensure_row_visible(0, 0, 5), 0);
            assert_eq!(l.ensure_row_visible(3, 1, 0), 3);
        }

        #[test]
        fn max_offset_is_zero_when_everything_fits() {
            let l = layout(vec![1, 1, 1]);

            assert_eq!(l.max_row_offset(5), 0);
        }

        #[test]
        fn max_offset_accounts_for_tall_rows() {
            let l = layout(vec![1, 1, 1, 1, 1, 1]);

            assert_eq!(l.max_row_offset(3), 3);
        }

        #[test]
        fn max_offset_with_one_huge_row_floors_at_last() {
            let l = layout(vec![10]);

            assert_eq!(l.max_row_offset(4), 0);
        }

        #[test]
        fn max_offset_with_mixed_tall_rows_floors_at_last() {
            let l = layout(vec![1, 1, 5]);

            assert_eq!(l.max_row_offset(4), 2);
        }

        #[test]
        fn scroll_to_cursor_uses_line_positions() {
            let l = layout(vec![2; 100]);

            assert_eq!(l.scroll_offset_for_center(50, 20), 45);
            assert_eq!(l.scroll_offset_for_top(50, 20), 50);
            assert_eq!(l.scroll_offset_for_bottom(50, 20), 41);
        }

        #[test]
        fn center_keeps_target_visible_after_a_tall_preceding_row() {
            let l = layout(vec![15, 1]);

            assert_eq!(l.scroll_offset_for_center(1, 10), 1);
        }

        #[test]
        fn bottom_keeps_target_visible_after_a_tall_preceding_row() {
            let l = layout(vec![15, 1]);

            assert_eq!(l.scroll_offset_for_bottom(1, 10), 1);
        }

        #[test]
        fn viewport_changes_layout_key() {
            let key = WrappedCellLayoutKey::default();

            assert_ne!(
                key.with_viewport(&[0, 1], &[4, 4]),
                key.with_viewport(&[1, 2], &[4, 4])
            );
            assert_ne!(
                key.with_viewport(&[0, 1], &[4, 4]),
                key.with_viewport(&[0, 1], &[5, 4])
            );
        }
    }
}
