use crate::domain::explain_plan::{self, ExplainPlan};

#[derive(Debug, Clone, Default)]
pub struct ExplainContext {
    pub plan_text: Option<String>,
    pub error: Option<String>,
    pub is_analyze: bool,
    pub execution_time_ms: u64,
    pub scroll_offset: usize,
    pub baseline: Option<ExplainPlan>,
    pub current_parsed: Option<ExplainPlan>,
    pub compare_scroll_offset: usize,
}

impl ExplainContext {
    pub fn set_plan(&mut self, text: String, is_analyze: bool, execution_time_ms: u64) {
        self.current_parsed = Some(explain_plan::parse_explain_text(
            &text,
            is_analyze,
            execution_time_ms,
        ));
        self.plan_text = Some(text);
        self.error = None;
        self.is_analyze = is_analyze;
        self.execution_time_ms = execution_time_ms;
        self.scroll_offset = 0;
        self.compare_scroll_offset = 0;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.plan_text = None;
        self.current_parsed = None;
        self.scroll_offset = 0;
    }

    pub fn reset(&mut self) {
        let baseline = self.baseline.take();
        *self = Self::default();
        self.baseline = baseline;
    }

    pub fn save_baseline(&mut self) -> bool {
        if let Some(ref parsed) = self.current_parsed {
            self.baseline = Some(parsed.clone());
            true
        } else {
            false
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_no_content() {
        let ctx = ExplainContext::default();

        assert!(ctx.plan_text.is_none());
        assert!(ctx.error.is_none());
        assert!(!ctx.is_analyze);
        assert_eq!(ctx.execution_time_ms, 0);
        assert_eq!(ctx.scroll_offset, 0);
        assert_eq!(ctx.line_count(), 0);
        assert!(ctx.baseline.is_none());
        assert!(ctx.current_parsed.is_none());
        assert_eq!(ctx.compare_scroll_offset, 0);
    }

    #[test]
    fn set_plan_stores_text_and_clears_error() {
        let mut ctx = ExplainContext {
            error: Some("old error".to_string()),
            ..Default::default()
        };

        ctx.set_plan("Seq Scan on users".to_string(), false, 42);

        assert_eq!(ctx.plan_text.as_deref(), Some("Seq Scan on users"));
        assert!(ctx.error.is_none());
        assert!(!ctx.is_analyze);
        assert_eq!(ctx.execution_time_ms, 42);
        assert_eq!(ctx.scroll_offset, 0);
    }

    #[test]
    fn set_plan_populates_current_parsed() {
        let mut ctx = ExplainContext::default();

        ctx.set_plan(
            "Seq Scan on users  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            42,
        );

        let parsed = ctx.current_parsed.as_ref().unwrap();
        assert_eq!(parsed.total_cost, Some(100.0));
        assert_eq!(parsed.estimated_rows, Some(10));
        assert_eq!(parsed.top_node_type.as_deref(), Some("Seq Scan on users"));
    }

    #[test]
    fn set_plan_overwrites_current_parsed() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            0,
        );

        ctx.set_plan(
            "Index Scan  (cost=0.00..5.00 rows=1 width=32)".to_string(),
            false,
            0,
        );

        let parsed = ctx.current_parsed.as_ref().unwrap();
        assert_eq!(parsed.total_cost, Some(5.0));
    }

    #[test]
    fn set_plan_with_analyze_flag() {
        let mut ctx = ExplainContext::default();

        ctx.set_plan("Seq Scan (actual)".to_string(), true, 100);

        assert!(ctx.is_analyze);
        assert_eq!(ctx.execution_time_ms, 100);
    }

    #[test]
    fn set_error_stores_error_and_clears_plan() {
        let mut ctx = ExplainContext {
            plan_text: Some("old plan".to_string()),
            ..Default::default()
        };

        ctx.set_error("syntax error".to_string());

        assert_eq!(ctx.error.as_deref(), Some("syntax error"));
        assert!(ctx.plan_text.is_none());
        assert_eq!(ctx.scroll_offset, 0);
    }

    #[test]
    fn set_error_clears_current_parsed() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            0,
        );
        assert!(ctx.current_parsed.is_some());

        ctx.set_error("error".to_string());

        assert!(ctx.current_parsed.is_none());
    }

    #[test]
    fn reset_clears_everything_except_baseline() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            true,
            50,
        );
        ctx.save_baseline();
        ctx.scroll_offset = 10;
        ctx.compare_scroll_offset = 5;

        ctx.reset();

        assert!(ctx.plan_text.is_none());
        assert!(ctx.error.is_none());
        assert!(!ctx.is_analyze);
        assert_eq!(ctx.execution_time_ms, 0);
        assert_eq!(ctx.scroll_offset, 0);
        assert_eq!(ctx.compare_scroll_offset, 0);
        assert!(ctx.current_parsed.is_none());
        assert!(ctx.baseline.is_some());
    }

    #[test]
    fn save_baseline_with_plan() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            0,
        );

        assert!(ctx.save_baseline());
        assert!(ctx.baseline.is_some());
        assert_eq!(ctx.baseline.as_ref().unwrap().total_cost, Some(100.0));
    }

    #[test]
    fn save_baseline_without_plan() {
        let mut ctx = ExplainContext::default();

        assert!(!ctx.save_baseline());
        assert!(ctx.baseline.is_none());
    }

    #[test]
    fn save_baseline_overwrites_previous() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan(
            "Seq Scan  (cost=0.00..100.00 rows=10 width=32)".to_string(),
            false,
            0,
        );
        ctx.save_baseline();

        ctx.set_plan(
            "Index Scan  (cost=0.00..5.00 rows=1 width=32)".to_string(),
            false,
            0,
        );
        ctx.save_baseline();

        assert_eq!(ctx.baseline.as_ref().unwrap().total_cost, Some(5.0));
    }

    #[test]
    fn line_count_with_plan() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan("line1\nline2\nline3".to_string(), false, 0);

        assert_eq!(ctx.line_count(), 3);
    }

    #[test]
    fn line_count_with_error() {
        let mut ctx = ExplainContext::default();
        ctx.set_error("err line1\nerr line2".to_string());

        assert_eq!(ctx.line_count(), 2);
    }

    #[test]
    fn set_plan_resets_scroll_offset() {
        let mut ctx = ExplainContext {
            scroll_offset: 15,
            ..Default::default()
        };

        ctx.set_plan("new plan".to_string(), false, 0);

        assert_eq!(ctx.scroll_offset, 0);
    }

    #[test]
    fn set_plan_resets_compare_scroll_offset() {
        let mut ctx = ExplainContext {
            compare_scroll_offset: 10,
            ..Default::default()
        };

        ctx.set_plan("new plan".to_string(), false, 0);

        assert_eq!(ctx.compare_scroll_offset, 0);
    }
}
