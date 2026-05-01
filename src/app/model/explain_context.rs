use std::collections::VecDeque;

use crate::domain::explain_plan::{self, ExplainPlan};
use crate::model::sql_editor::modal::sql_modal_visible_rows;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotSource {
    AutoPrevious,
    AutoLatest,
}

impl SlotSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::AutoPrevious => "Previous",
            Self::AutoLatest => "Latest",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompareSlot {
    pub plan: ExplainPlan,
    pub query_snippet: String,
    pub full_query: String,
    pub source: SlotSource,
}

const MAX_EXPLAIN_HISTORY: usize = 10;

#[derive(Debug, Clone, Default)]
pub struct ExplainContext {
    plan_text: Option<String>,
    plan_query_snippet: Option<String>,
    error: Option<String>,
    is_analyze: bool,
    execution_time_ms: u64,
    scroll_offset: usize,

    left: Option<CompareSlot>,
    right: Option<CompareSlot>,
    compare_scroll_offset: usize,

    history: VecDeque<CompareSlot>,

    compare_viewport_height: Option<u16>,
    confirm_scroll_offset: usize,
}

impl ExplainContext {
    pub fn plan_text(&self) -> Option<&str> {
        self.plan_text.as_deref()
    }

    pub fn plan_query_snippet(&self) -> Option<&str> {
        self.plan_query_snippet.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn is_analyze(&self) -> bool {
        self.is_analyze
    }

    pub fn execution_time_ms(&self) -> u64 {
        self.execution_time_ms
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn left(&self) -> Option<&CompareSlot> {
        self.left.as_ref()
    }

    pub fn right(&self) -> Option<&CompareSlot> {
        self.right.as_ref()
    }

    pub fn history(&self) -> &VecDeque<CompareSlot> {
        &self.history
    }

    pub fn compare_scroll_offset(&self) -> usize {
        self.compare_scroll_offset
    }

    pub fn confirm_scroll_offset(&self) -> usize {
        self.confirm_scroll_offset
    }

    pub fn set_plan(
        &mut self,
        text: String,
        is_analyze: bool,
        execution_time_ms: u64,
        query: &str,
    ) {
        let parsed = explain_plan::parse_explain_text(&text, is_analyze, execution_time_ms);
        let snippet = query.lines().next().unwrap_or("").to_string();
        let plan_snippet = snippet.clone();

        let new_slot = CompareSlot {
            plan: parsed,
            query_snippet: snippet,
            full_query: query.to_string(),
            source: SlotSource::AutoLatest,
        };

        // Auto-advance: right → left
        self.left = self.right.take().map(|mut s| {
            s.source = SlotSource::AutoPrevious;
            s
        });
        self.history.push_front(new_slot);
        self.history.truncate(MAX_EXPLAIN_HISTORY);
        self.right = self.history.front().cloned();

        self.plan_text = Some(text);
        self.plan_query_snippet = Some(plan_snippet);
        self.error = None;
        self.is_analyze = is_analyze;
        self.execution_time_ms = execution_time_ms;
        self.scroll_offset = 0;
        self.compare_scroll_offset = 0;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.plan_text = None;
        self.scroll_offset = 0;
    }

    pub fn reset_confirm_scroll(&mut self) {
        self.confirm_scroll_offset = 0;
    }

    pub fn set_compare_viewport_height(&mut self, height: u16) {
        self.compare_viewport_height = Some(height);
    }

    pub fn scroll_confirm_to(&mut self, offset: usize) {
        self.confirm_scroll_offset = offset;
    }

    pub fn scroll_plan_to(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    pub fn scroll_compare_to(&mut self, offset: usize) {
        self.compare_scroll_offset = offset;
    }

    pub fn right_full_query(&self) -> Option<&str> {
        self.right.as_ref().map(|slot| slot.full_query.as_str())
    }

    pub fn reset(&mut self) {
        let left = self.left.take();
        let right = self.right.take();
        let history = std::mem::take(&mut self.history);

        *self = Self::default();

        self.left = left;
        self.right = right;
        self.history = history;
    }

    pub fn line_count(&self) -> usize {
        if let Some(ref text) = self.plan_text {
            text.lines().count()
        } else if let Some(ref err) = self.error {
            err.lines().count()
        } else {
            0
        }
    }

    // blank + verdict + blank + reasons(3) + blank + separator + blank + slot header + detail + thin_sep
    const COMPARE_HEADER_OVERHEAD_FULL: usize = 12;
    // slot header + query detail + thin_sep + plan lines (no verdict section)
    const COMPARE_HEADER_OVERHEAD_PARTIAL: usize = 3;

    pub fn compare_line_count(&self) -> usize {
        match (&self.left, &self.right) {
            (Some(l), Some(r)) => {
                let l_lines = l.plan.raw_text.lines().count();
                let r_lines = r.plan.raw_text.lines().count();
                Self::COMPARE_HEADER_OVERHEAD_FULL + l_lines.max(r_lines)
            }
            (Some(s), None) | (None, Some(s)) => {
                Self::COMPARE_HEADER_OVERHEAD_PARTIAL + s.plan.raw_text.lines().count()
            }
            (None, None) => 0,
        }
    }

    pub fn modal_inner_height(terminal_height: u16) -> usize {
        sql_modal_visible_rows(terminal_height)
    }

    pub fn compare_max_scroll(&self, terminal_height: u16) -> usize {
        let viewport = self
            .compare_viewport_height
            .map_or_else(|| Self::modal_inner_height(terminal_height), |h| h as usize);
        self.compare_line_count().saturating_sub(viewport)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ExplainContext {
    pub fn set_plan_text_for_test(&mut self, plan_text: Option<String>) {
        self.plan_text = plan_text;
    }

    pub fn set_error_for_test(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub fn set_compare_slots_for_test(
        &mut self,
        left: Option<CompareSlot>,
        right: Option<CompareSlot>,
    ) {
        self.left = left;
        self.right = right;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_no_content() {
        let ctx = ExplainContext::default();

        assert!(ctx.plan_text().is_none());
        assert!(ctx.error().is_none());
        assert!(ctx.left().is_none());
        assert!(ctx.right().is_none());
        assert!(ctx.history().is_empty());
    }

    #[test]
    fn first_explain_sets_right_only() {
        let mut ctx = ExplainContext::default();

        ctx.set_plan(
            "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            42,
            "SELECT * FROM users",
        );

        assert!(ctx.left().is_none());
        assert!(ctx.right().is_some());
        assert_eq!(ctx.right().unwrap().plan.total_cost, Some(100.0));
        assert_eq!(ctx.right().unwrap().query_snippet, "SELECT * FROM users");
        assert_eq!(ctx.right().unwrap().source, SlotSource::AutoLatest);
    }

    #[test]
    fn second_explain_auto_advances_right_to_left() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            0,
            "SELECT * FROM users",
        );

        ctx.set_plan(
            "Index Scan  (cost=0.00..5.00 rows=1 width=32)".to_string(),
            false,
            0,
            "SELECT * FROM users WHERE id = 1",
        );

        assert!(ctx.left().is_some());
        assert_eq!(ctx.left().unwrap().plan.total_cost, Some(100.0));
        assert_eq!(ctx.left().unwrap().source, SlotSource::AutoPrevious);
        assert_eq!(ctx.right().unwrap().plan.total_cost, Some(5.0));
        assert_eq!(ctx.right().unwrap().source, SlotSource::AutoLatest);
    }

    #[test]
    fn history_stores_all_explains() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "A  (cost=0.00..10.00 rows=1 width=32)".to_string(),
            false,
            0,
            "A",
        );
        ctx.set_plan(
            "B  (cost=0.00..20.00 rows=2 width=32)".to_string(),
            false,
            0,
            "B",
        );
        ctx.set_plan(
            "C  (cost=0.00..30.00 rows=3 width=32)".to_string(),
            false,
            0,
            "C",
        );

        assert_eq!(ctx.history().len(), 3);
        assert_eq!(ctx.history()[0].query_snippet, "C");
        assert_eq!(ctx.history()[2].query_snippet, "A");
    }

    #[test]
    fn reset_preserves_compare_state_and_history() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "A  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            0,
            "A",
        );
        ctx.set_plan(
            "B  (cost=0.00..50.00 rows=5 width=32)".to_string(),
            false,
            0,
            "B",
        );
        ctx.scroll_plan_to(10);
        ctx.scroll_compare_to(5);

        ctx.reset();

        assert!(ctx.plan_text().is_none());
        assert!(ctx.error().is_none());
        assert_eq!(ctx.scroll_offset(), 0);
        assert_eq!(ctx.compare_scroll_offset(), 0);
        assert!(ctx.left().is_some());
        assert!(ctx.right().is_some());
        assert_eq!(ctx.history().len(), 2);
    }

    #[test]
    fn set_error_does_not_affect_compare_slots() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "A  (cost=0.00..10.00 rows=1 width=32)".to_string(),
            false,
            0,
            "A",
        );

        ctx.set_error("some error".to_string());

        assert!(ctx.right().is_some());
    }

    #[test]
    fn history_truncates_at_max() {
        let mut ctx = ExplainContext::default();
        for i in 0..15 {
            ctx.set_plan(
                format!("Scan  (cost=0.00..{i}.00 rows=1 width=32)"),
                false,
                0,
                &format!("Q{i}"),
            );
        }

        assert_eq!(ctx.history().len(), MAX_EXPLAIN_HISTORY);
    }

    #[test]
    fn set_plan_stores_query_snippet_first_line_only() {
        let mut ctx = ExplainContext::default();

        ctx.set_plan(
            "Seq Scan  (cost=0.00..10.00 rows=1 width=32)".to_string(),
            false,
            0,
            "SELECT *\nFROM users\nWHERE id = 1",
        );

        assert_eq!(ctx.right().unwrap().query_snippet, "SELECT *");
    }

    #[test]
    fn line_count_with_plan() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan("line1\nline2\nline3".to_string(), false, 0, "Q");

        assert_eq!(ctx.line_count(), 3);
    }

    #[test]
    fn line_count_with_error() {
        let mut ctx = ExplainContext::default();
        ctx.set_error("err1\nerr2".to_string());

        assert_eq!(ctx.line_count(), 2);
    }

    #[test]
    fn modal_inner_height_uses_shared_visible_rows_contract() {
        assert_eq!(ExplainContext::modal_inner_height(0), 8);
        assert_eq!(ExplainContext::modal_inner_height(1), 1);
    }
}
