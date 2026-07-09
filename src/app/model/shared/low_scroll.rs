//! Low Scroll Mode layout engine.
//!
//! When Low Scroll Mode is active and horizontal scrolling is disabled, the
//! entire result set must fit inside the pane width without a horizontal
//! scrollbar. Columns are shrunk to fit and cell text is wrapped, expanding
//! rows vertically so the whole cell content is visible.
//!
//! This module is pure: it takes widths/text/available space and returns the
//! computed layout. Rendering lives in the UI layer; this is the geometry.

use unicode_width::UnicodeWidthStr;

/// Number of terminal lines `text` occupies when wrapped at `width` display
/// cells. Each explicit newline starts a new line; long lines wrap. A width
/// of zero yields zero lines (nothing can be rendered).
///
/// Mirrors `text_utils::wrapped_line_count` so the app layer stays independent
/// of the UI crate.
fn wrapped_line_count(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }

    text.lines().fold(0u16, |acc, line| {
        let w = UnicodeWidthStr::width(line).min(u16::MAX as usize) as u16;
        let wrapped = w.max(1).div_ceil(width);
        acc.saturating_add(wrapped)
    })
}

/// Settings for Low Scroll Mode, mirrored in the config file and settings UI.
///
/// The `Default` (`allow_horizontal_scroll: false`, `max_lines_per_row: None`)
/// is deliberate: Low Scroll Mode defaults to actually lowering scroll —
/// columns shrink to fit and text wraps, with no per-row line cap. The user
/// can opt back into horizontal scrolling from the settings panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LowScrollSettings {
    /// When `true`, column widths behave exactly as before (scroll allowed).
    /// When `false`, columns are shrunk to fit the pane and text is wrapped.
    pub allow_horizontal_scroll: bool,
    /// Caps the number of rendered lines per row. `None` means no cap: a row
    /// grows as tall as its widest wrapped cell needs.
    pub max_lines_per_row: Option<u16>,
}

impl LowScrollSettings {
    /// Effective wrap width for a column = column width minus the cell padding.
    /// A column narrower than its padding renders an empty wrapped body.
    #[must_use]
    pub fn effective_wrap_width(col_width: u16, padding: u16) -> u16 {
        col_width.saturating_sub(padding).max(1)
    }

    /// Number of lines a cell occupies after wrapping at `col_width`, capped
    /// by `max_lines_per_row`. `capped` is true when the cap truncated content.
    #[must_use]
    pub fn wrapped_cell_lines(text: &str, col_width: u16, padding: u16) -> u16 {
        let wrap_width = Self::effective_wrap_width(col_width, padding);
        wrapped_line_count(text, wrap_width)
    }
}

/// The computed layout for a single column in Low Scroll Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LowScrollColumn {
    pub width: u16,
}

/// The computed layout for a single row in Low Scroll Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LowScrollRow {
    /// Rendered height in terminal lines, already clamped to the row cap.
    pub height: u16,
    /// True when any cell in this row was truncated by the line cap.
    pub truncated: bool,
}

/// Complete Low Scroll Mode layout: one width per column, one height per row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LowScrollLayout {
    pub columns: Vec<LowScrollColumn>,
    pub rows: Vec<LowScrollRow>,
}

/// Identifies the inputs a measured layout was computed for, so the renderer
/// can reuse a cached layout across frames instead of re-measuring every row.
///
/// The per-row heights only change when one of these changes: a new result
/// (`result_generation`), a pane resize (`inner_width`, which drives the
/// shrunk column widths), or a Low Scroll setting toggle. During plain
/// vertical scrolling none of these move, so the cached vector is reused and
/// the O(rows) measurement is skipped entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LowScrollLayoutKey {
    pub result_generation: u64,
    pub inner_width: u16,
    pub allow_horizontal_scroll: bool,
    pub max_lines_per_row: Option<u16>,
}

/// Per-row line heights the scroll reducer needs to keep the active row visible.
///
/// Measured by the renderer (which owns the pane geometry) and written back
/// through `RenderOutput` so the reducer can do line-based — not
/// row-count-based — visibility math.
///
/// `row_heights[i]` is the rendered height in terminal lines of absolute row
/// `i` (after the per-row cap and any dynamic screen-height clamp). It is
/// indexed by absolute row index, matching `QueryResult::rows`.
///
/// `line_prefix` is a running total of clamped row heights: `line_prefix[i]` is
/// the number of terminal lines occupied by rows `0..i` (so `line_prefix[0]` is
/// `0` and `line_prefix[n]` is the grand total). It turns the reducer's
/// visibility math — total lines, "lines above a row", and the maximum scroll
/// offset — into O(1)/O(log n) lookups instead of O(n) walks over every row,
/// which is what made scrolling a multi-million-row result unusable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MeasuredLowScrollLayout {
    /// Per-row rendered line heights, indexed by absolute row.
    pub row_heights: Vec<u16>,
    /// Prefix sums of the clamped row heights; length `row_heights.len() + 1`.
    pub line_prefix: Vec<usize>,
    /// The inputs this layout was measured for (cache-validity key).
    pub key: LowScrollLayoutKey,
}

impl MeasuredLowScrollLayout {
    /// Build a measured layout from per-row heights, precomputing the prefix
    /// sums used for line-based scroll math.
    #[must_use]
    pub fn new(row_heights: Vec<u16>, key: LowScrollLayoutKey) -> Self {
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

    /// Total rendered lines across every row (O(1)).
    #[must_use]
    pub fn total_lines(&self) -> usize {
        self.line_prefix.last().copied().unwrap_or(0)
    }

    /// Rendered height in terminal lines of absolute row `row`, defaulting to
    /// `1` for out-of-range indices so callers can treat missing rows as
    /// single-line without special-casing.
    #[must_use]
    pub fn row_height(&self, row: usize) -> usize {
        self.row_heights.get(row).map_or(1, |&h| h.max(1) as usize)
    }

    /// Lines occupied by all rows before `row` (i.e. rows `0..row`), clamped to
    /// the row count (O(1)).
    fn lines_before(&self, row: usize) -> usize {
        let idx = row.min(self.row_heights.len());
        self.line_prefix[idx]
    }

    /// Clamp a `scroll_offset` (in rows) so `target_row` stays visible given the
    /// per-row line heights and the available pane line budget.
    ///
    /// Line-based analogue of the normal table's row-count visibility math:
    /// because rows have variable heights, a row counts as `row_height(row)`
    /// lines, not 1.
    ///
    /// - If the target is already on screen, the offset is returned unchanged.
    /// - If below the pane, the offset advances the *minimum* amount so the
    ///   target's last line touches the pane bottom (mirroring the normal
    ///   table's `row - visible + 1`).
    /// - If above the pane, the offset retreats so the target is the first
    ///   visible row.
    ///
    /// O(log n): the "lines above" total and the minimal advance are found via
    /// the prefix sums rather than by walking rows.
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
            // Above the viewport: make it the first visible row.
            return target;
        }

        let lines_above = self.lines_before(target) - self.lines_before(scroll_offset);
        let target_height = self.row_height(target);
        if lines_above + target_height <= line_budget {
            // Already visible.
            return scroll_offset;
        }

        // Below the viewport: advance the minimum so the rows `offset..=target`
        // fit the budget, i.e. the smallest `offset` with
        // `line_prefix[offset] >= line_prefix[target + 1] - line_budget`.
        let span_end = self.line_prefix[target + 1];
        let threshold = span_end.saturating_sub(line_budget);
        self.line_prefix
            .partition_point(|&lines| lines < threshold)
            .min(target)
    }

    /// Maximum row scroll offset given variable row heights: the largest offset
    /// at which the trailing content still fills (or underfills) the pane.
    /// Equivalent to `total_rows.saturating_sub(visible_rows)` in the normal
    /// row-count world.
    ///
    /// O(log n): a binary search over the prefix sums for the first offset whose
    /// trailing lines fit the budget.
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
        // Smallest offset with `total - line_prefix[offset] <= line_budget`.
        let threshold = total - line_budget;
        self.line_prefix
            .partition_point(|&lines| lines < threshold)
            .min(n - 1)
    }
}

impl LowScrollLayout {
    /// Total horizontal width of all columns including the separators between
    /// them (one cell per gap, matching the existing table renderer).
    #[must_use]
    pub fn total_width(&self) -> u16 {
        total_width_with_separators(&self.column_widths())
    }

    fn column_widths(&self) -> Vec<u16> {
        self.columns.iter().map(|c| c.width).collect()
    }
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

/// Compute the Low Scroll Mode layout for a full result set.
///
/// 1. Columns are proportionally shrunk from their ideal widths so the total
///    (including separators) fits exactly inside `available_width`.
/// 2. Each cell is wrapped at its column's width; the row height is the max
///    across its cells, clamped to `settings.max_lines_per_row`.
///
/// `padding` is the per-cell horizontal padding (matches `PADDING` in
/// `text_utils`), subtracted from a column width to get the wrap width.
#[must_use]
pub fn compute_layout(
    headers: &[String],
    rows: &[Vec<String>],
    ideal_widths: &[u16],
    available_width: u16,
    settings: &LowScrollSettings,
    padding: u16,
) -> LowScrollLayout {
    if ideal_widths.is_empty() {
        return LowScrollLayout::default();
    }

    let column_widths = shrink_columns_to_fit(ideal_widths, available_width);
    let columns: Vec<LowScrollColumn> = column_widths
        .iter()
        .map(|&w| LowScrollColumn { width: w })
        .collect();

    let row_cap = settings.max_lines_per_row;

    let computed_rows: Vec<LowScrollRow> = rows
        .iter()
        .map(|row| row_layout(row, &column_widths, padding, row_cap))
        .collect();

    // Header always counts as a single-line row visually; it is rendered
    // separately by the table widget and is not part of `rows`.
    let _ = headers;

    LowScrollLayout {
        columns,
        rows: computed_rows,
    }
}

/// Shrink a set of ideal column widths so their total (with separators) fits
/// inside `available_width`, preserving relative proportions.
///
/// Columns that already fit are returned unchanged. Columns never shrink below
/// `min_col_width` so a cell always has room for at least a couple of glyphs
/// (and the wrap width stays >= 1).
#[must_use]
pub fn shrink_columns_to_fit(ideal_widths: &[u16], available_width: u16) -> Vec<u16> {
    const MIN_COL_WIDTH: u16 = 4;

    let total = total_width_with_separators(ideal_widths);
    if total <= available_width || ideal_widths.is_empty() {
        return ideal_widths.to_vec();
    }

    // Budget for column content only (separators are fixed overhead).
    let separator_overhead = ideal_widths.len().saturating_sub(1) as u16;
    let content_budget = available_width.saturating_sub(separator_overhead);

    distribute_budget(ideal_widths, content_budget, MIN_COL_WIDTH)
}

/// Distribute `budget` across columns proportionally to their ideal widths,
/// never dropping any column below `min_width`. Leftover cells from rounding
/// are given to the columns that had the most ideal width.
fn distribute_budget(ideal: &[u16], budget: u16, min_width: u16) -> Vec<u16> {
    let n = ideal.len();
    if n == 0 {
        return Vec::new();
    }

    let min_total = min_width.saturating_mul(n as u16);
    // If even minimums do not fit, clamp every column to the minimum. Wrapping
    // then makes rows tall; the data is still fully visible vertically.
    if budget <= min_total {
        return vec![min_width; n];
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

    // Redistribute: proportional scaling can over- or under-shoot the budget
    // because of integer rounding and the min-width floor. Reconcile in a
    // second pass so the total matches the budget exactly when possible.
    reconcile_to_budget(&mut widths, budget, min_width);

    widths
}

/// Adjust per-column widths so their sum equals `budget`, adjusting the
/// columns with the most slack first (largest gap between current and ideal).
fn reconcile_to_budget(widths: &mut [u16], budget: u16, min_width: u16) {
    let target = budget;

    loop {
        let current: u32 = widths.iter().map(|&w| u32::from(w)).sum();
        if current == u32::from(target) {
            break;
        }

        if current > u32::from(target) {
            // Over budget: shave from the widest column that can shrink.
            let Some(widest) = widest_shrinkable_index(widths, min_width) else {
                break;
            };
            widths[widest] -= 1;
        } else {
            // Under budget: give to the narrowest column.
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

/// Compute the layout (height + truncation flag) for a single row given the
/// final column widths.
pub fn row_layout(
    row: &[String],
    column_widths: &[u16],
    padding: u16,
    row_cap: Option<u16>,
) -> LowScrollRow {
    // Without a cap, height is the maximum wrapped line count across cells.
    if let Some(cap) = row_cap {
        let overflowed = overflowed_any(row, column_widths, padding, cap);
        let raw_max = max_wrapped_lines(row, column_widths, padding);
        return LowScrollRow {
            height: raw_max.min(cap).max(1),
            truncated: overflowed,
        };
    }

    LowScrollRow {
        height: max_wrapped_lines(row, column_widths, padding).max(1),
        truncated: false,
    }
}

/// Maximum wrapped line count across the cells of a row.
fn max_wrapped_lines(row: &[String], column_widths: &[u16], padding: u16) -> u16 {
    row.iter()
        .enumerate()
        .map(|(col_idx, cell)| {
            column_widths.get(col_idx).map_or(1, |&col_width| {
                LowScrollSettings::wrapped_cell_lines(cell, col_width, padding)
            })
        })
        .max()
        .unwrap_or(1)
}

/// True when any cell in the row wraps to more lines than `cap`.
fn overflowed_any(row: &[String], column_widths: &[u16], padding: u16, cap: u16) -> bool {
    row.iter().enumerate().any(|(col_idx, cell)| {
        column_widths.get(col_idx).is_some_and(|&col_width| {
            LowScrollSettings::wrapped_cell_lines(cell, col_width, padding) > cap
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
            let s = LowScrollSettings::default();

            assert!(!s.allow_horizontal_scroll);
            assert_eq!(s.max_lines_per_row, None);
        }

        #[rstest]
        #[case(10, 2, 8)]
        #[case(2, 2, 1)]
        #[case(1, 2, 1)]
        #[case(0, 2, 1)]
        fn wrap_width_subtracts_padding(#[case] col: u16, #[case] pad: u16, #[case] expected: u16) {
            assert_eq!(LowScrollSettings::effective_wrap_width(col, pad), expected);
        }

        #[rstest]
        #[case("hello", 10, 2, 1)]
        // wrap width = 8 - 2 = 6; "hello world foo" is 15 cells -> ceil(15/6) = 3
        #[case("hello world foo", 8, 2, 3)]
        #[case("a\nb\nc", 10, 2, 3)] // explicit newlines count as lines
        #[case("", 10, 2, 0)]
        fn wrapped_cell_lines_counts_display(
            #[case] text: &str,
            #[case] col: u16,
            #[case] pad: u16,
            #[case] expected: u16,
        ) {
            assert_eq!(
                LowScrollSettings::wrapped_cell_lines(text, col, pad),
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

            // total = 51, available = 21, separators = 1, content budget = 20
            // ratio: 40/(50) * 20 = 16, 10/50 * 20 = 4
            let result = shrink_columns_to_fit(&widths, 21);

            assert_eq!(result.len(), 2);
            assert!(result[0] > result[1]);
            let total = result[0] + result[1] + 1; // + separator
            assert_eq!(total, 21);
        }

        #[test]
        fn never_below_min_width() {
            let widths = vec![100, 100, 100, 100];

            // available = 10, separators = 3, budget = 7 < min_total 16
            let result = shrink_columns_to_fit(&widths, 10);

            assert_eq!(result, vec![4, 4, 4, 4]);
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

            // budget = 30 - 2 separators = 28, 28/3 each ≈ 9
            let result = shrink_columns_to_fit(&widths, 30);

            assert_eq!(result, vec![10, 9, 9]);
            let total = result.iter().sum::<u16>() + 2;
            assert_eq!(total, 30);
        }
    }

    mod compute_layout {
        use super::*;

        fn settings(allow_scroll: bool, cap: Option<u16>) -> LowScrollSettings {
            LowScrollSettings {
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

            // col width 10 - pad 2 = wrap 8; "this is a longer value" ~ 21 chars -> 3 lines
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
            let rows = vec![vec!["".to_string()]];
            let ideal = vec![12];

            let layout = compute_layout(&headers, &rows, &ideal, 12, &settings(false, None), 2);

            assert_eq!(layout.rows[0].height, 1);
        }
    }

    mod total_width {
        use super::*;

        #[test]
        fn includes_separators() {
            let layout = LowScrollLayout {
                columns: vec![LowScrollColumn { width: 10 }, LowScrollColumn { width: 20 }],
                rows: vec![],
            };

            assert_eq!(layout.total_width(), 31);
        }

        #[test]
        fn single_column_no_separator() {
            let layout = LowScrollLayout {
                columns: vec![LowScrollColumn { width: 15 }],
                rows: vec![],
            };

            assert_eq!(layout.total_width(), 15);
        }
    }

    mod line_based_visibility {
        use super::*;

        fn layout(heights: Vec<u16>) -> MeasuredLowScrollLayout {
            MeasuredLowScrollLayout::new(heights, LowScrollLayoutKey::default())
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
            // Zero-height rows are clamped to a single line each.
            assert_eq!(layout(vec![0, 0]).total_lines(), 2);
            assert_eq!(layout(vec![]).total_lines(), 0);
        }

        #[test]
        fn ensure_visible_returns_unchanged_when_already_visible() {
            // Each row 1 line, budget 5, target 2, offset 0: visible.
            let l = layout(vec![1, 1, 1, 1, 1]);

            assert_eq!(l.ensure_row_visible(0, 2, 5), 0);
        }

        #[test]
        fn ensure_visible_advances_minimally_when_below() {
            // Rows of 1 line each, budget 3, target 4, offset 0.
            // Normal table: offset = 4 - 3 + 1 = 2.
            let l = layout(vec![1, 1, 1, 1, 1, 1]);

            assert_eq!(l.ensure_row_visible(0, 4, 3), 2);
        }

        #[test]
        fn ensure_visible_accounts_for_tall_rows() {
            // Row 0 is 5 lines, row 1 is 1 line, budget 4.
            // Target row 1: lines_above (row 0) = 5, +1 = 6 > 4 → advance to 1.
            let l = layout(vec![5, 1]);

            assert_eq!(l.ensure_row_visible(0, 1, 4), 1);
        }

        #[test]
        fn ensure_visible_pins_target_to_top_when_too_tall() {
            // A single row of 10 lines with budget 4: target 0 is too tall.
            // It can't fit, so offset stays 0 (it fills the pane).
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
            // 6 rows of 1 line each, budget 3 → can scroll until 2 rows left.
            let l = layout(vec![1, 1, 1, 1, 1, 1]);

            assert_eq!(l.max_row_offset(3), 3);
        }

        #[test]
        fn max_offset_with_one_huge_row_floors_at_last() {
            // A single 10-line row with budget 4: still offset 0.
            let l = layout(vec![10]);

            assert_eq!(l.max_row_offset(4), 0);
        }

        #[test]
        fn max_offset_with_mixed_tall_rows_floors_at_last() {
            // Last row (5 lines) alone overflows a 4-line budget: floor at the
            // last row so at least its top is shown.
            let l = layout(vec![1, 1, 5]);

            assert_eq!(l.max_row_offset(4), 2);
        }
    }
}
