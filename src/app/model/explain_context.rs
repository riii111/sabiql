use std::collections::VecDeque;

use crate::domain::explain_plan::{self, ExplainPlan};

#[derive(Debug, Clone, PartialEq)]
pub enum SlotSource {
    AutoPrevious,
    AutoLatest,
    Manual,
    Pinned,
}

impl SlotSource {
    pub fn label(&self) -> &'static str {
        match self {
            SlotSource::AutoPrevious => "Previous",
            SlotSource::AutoLatest => "Latest",
            SlotSource::Manual => "Manual",
            SlotSource::Pinned => "Pinned",
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
    pub plan_text: Option<String>,
    pub plan_query_snippet: Option<String>,
    pub error: Option<String>,
    pub is_analyze: bool,
    pub execution_time_ms: u64,
    pub scroll_offset: usize,

    pub left: Option<CompareSlot>,
    pub right: Option<CompareSlot>,
    pub left_pinned: bool,
    pub compare_scroll_offset: usize,

    pub history: VecDeque<CompareSlot>,

    pub left_history_cursor: usize,
    pub right_history_cursor: usize,
    pub compare_viewport_height: Option<u16>,
    pub confirm_scroll_offset: usize,
}

impl ExplainContext {
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

        // Auto-advance: right → left (unless left is pinned)
        if !self.left_pinned {
            self.left = self.right.take().map(|mut s| {
                s.source = SlotSource::AutoPrevious;
                s
            });
        }
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
        self.left_history_cursor = 0;
        self.right_history_cursor = 0;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.plan_text = None;
        self.scroll_offset = 0;
    }

    pub fn reset(&mut self) {
        let left = self.left.take();
        let right = self.right.take();
        let left_pinned = self.left_pinned;
        let history = std::mem::take(&mut self.history);

        *self = Self::default();

        self.left = left;
        self.right = right;
        self.left_pinned = left_pinned;
        self.history = history;
    }

    pub fn pin_left(&mut self) -> bool {
        if let Some(ref right) = self.right {
            self.left = Some(CompareSlot {
                source: SlotSource::Pinned,
                ..right.clone()
            });
            self.left_pinned = true;
            true
        } else {
            false
        }
    }

    pub fn select_left(&mut self, index: usize) -> bool {
        if let Some(entry) = self.history.get(index) {
            self.left = Some(CompareSlot {
                source: SlotSource::Manual,
                ..entry.clone()
            });
            self.left_pinned = true;
            true
        } else {
            false
        }
    }

    pub fn select_right(&mut self, index: usize) -> bool {
        if let Some(entry) = self.history.get(index) {
            self.right = Some(CompareSlot {
                source: SlotSource::Manual,
                ..entry.clone()
            });
            true
        } else {
            false
        }
    }

    pub fn cycle_left_slot(&mut self) -> bool {
        let len = self.history.len();
        if len == 0 {
            return false;
        }
        let cursor = (self.left_history_cursor + 1) % len;
        self.left_history_cursor = cursor;
        self.select_left(cursor);
        self.compare_scroll_offset = 0;
        true
    }

    pub fn cycle_right_slot(&mut self) -> bool {
        let len = self.history.len();
        if len == 0 {
            return false;
        }
        let cursor = (self.right_history_cursor + 1) % len;
        self.right_history_cursor = cursor;
        self.select_right(cursor);
        self.compare_scroll_offset = 0;
        true
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
        const MODAL_HEIGHT_PERCENT: usize = 60;
        // border(2) + separator(1) + status(1) + padding(1)
        const MODAL_CHROME_LINES: usize = 5;
        (terminal_height as usize * MODAL_HEIGHT_PERCENT / 100).saturating_sub(MODAL_CHROME_LINES)
    }

    pub fn compare_max_scroll(&self, terminal_height: u16) -> usize {
        let viewport = self
            .compare_viewport_height
            .map(|h| h as usize)
            .unwrap_or_else(|| Self::modal_inner_height(terminal_height));
        self.compare_line_count().saturating_sub(viewport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_no_content() {
        let ctx = ExplainContext::default();

        assert!(ctx.plan_text.is_none());
        assert!(ctx.error.is_none());
        assert!(ctx.left.is_none());
        assert!(ctx.right.is_none());
        assert!(!ctx.left_pinned);
        assert!(ctx.history.is_empty());
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

        assert!(ctx.left.is_none());
        assert!(ctx.right.is_some());
        assert_eq!(ctx.right.as_ref().unwrap().plan.total_cost, Some(100.0));
        assert_eq!(
            ctx.right.as_ref().unwrap().query_snippet,
            "SELECT * FROM users"
        );
        assert_eq!(ctx.right.as_ref().unwrap().source, SlotSource::AutoLatest);
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

        assert!(ctx.left.is_some());
        assert_eq!(ctx.left.as_ref().unwrap().plan.total_cost, Some(100.0));
        assert_eq!(ctx.left.as_ref().unwrap().source, SlotSource::AutoPrevious);
        assert_eq!(ctx.right.as_ref().unwrap().plan.total_cost, Some(5.0));
        assert_eq!(ctx.right.as_ref().unwrap().source, SlotSource::AutoLatest);
    }

    #[test]
    fn pin_left_prevents_auto_advance() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "A  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            0,
            "A",
        );
        ctx.pin_left();

        ctx.set_plan(
            "B  (cost=0.00..50.00 rows=5 width=32)".to_string(),
            false,
            0,
            "B",
        );

        assert_eq!(ctx.left.as_ref().unwrap().plan.total_cost, Some(100.0));
        assert_eq!(ctx.left.as_ref().unwrap().source, SlotSource::Pinned);
        assert_eq!(ctx.right.as_ref().unwrap().plan.total_cost, Some(50.0));
    }

    #[test]
    fn pin_left_without_right_returns_false() {
        let mut ctx = ExplainContext::default();

        assert!(!ctx.pin_left());
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

        assert_eq!(ctx.history.len(), 3);
        assert_eq!(ctx.history[0].query_snippet, "C");
        assert_eq!(ctx.history[2].query_snippet, "A");
    }

    #[test]
    fn select_left_from_history() {
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

        ctx.select_left(2); // select A (oldest)

        assert_eq!(ctx.left.as_ref().unwrap().query_snippet, "A");
        assert_eq!(ctx.left.as_ref().unwrap().source, SlotSource::Manual);
        assert!(ctx.left_pinned);
    }

    #[test]
    fn select_right_from_history() {
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

        ctx.select_right(1); // select A

        assert_eq!(ctx.right.as_ref().unwrap().query_snippet, "A");
        assert_eq!(ctx.right.as_ref().unwrap().source, SlotSource::Manual);
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
        ctx.pin_left();
        ctx.scroll_offset = 10;
        ctx.compare_scroll_offset = 5;

        ctx.reset();

        assert!(ctx.plan_text.is_none());
        assert!(ctx.error.is_none());
        assert_eq!(ctx.scroll_offset, 0);
        assert_eq!(ctx.compare_scroll_offset, 0);
        assert!(ctx.left.is_some());
        assert!(ctx.right.is_some());
        assert!(ctx.left_pinned);
        assert_eq!(ctx.history.len(), 2);
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

        assert!(ctx.right.is_some());
    }

    #[test]
    fn history_truncates_at_max() {
        let mut ctx = ExplainContext::default();
        for i in 0..15 {
            ctx.set_plan(
                format!("Scan  (cost=0.00..{}.00 rows=1 width=32)", i),
                false,
                0,
                &format!("Q{}", i),
            );
        }

        assert_eq!(ctx.history.len(), MAX_EXPLAIN_HISTORY);
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

        assert_eq!(ctx.right.as_ref().unwrap().query_snippet, "SELECT *");
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
}
