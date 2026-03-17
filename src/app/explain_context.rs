#[derive(Debug, Clone, Default)]
pub struct ExplainContext {
    pub plan_text: Option<String>,
    pub error: Option<String>,
    pub is_analyze: bool,
    pub execution_time_ms: u64,
    pub scroll_offset: usize,
}

impl ExplainContext {
    pub fn set_plan(&mut self, text: String, is_analyze: bool, execution_time_ms: u64) {
        self.plan_text = Some(text);
        self.error = None;
        self.is_analyze = is_analyze;
        self.execution_time_ms = execution_time_ms;
        self.scroll_offset = 0;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.plan_text = None;
        self.scroll_offset = 0;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
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
    fn reset_clears_everything() {
        let mut ctx = ExplainContext::default();
        ctx.set_plan("plan".to_string(), true, 50);
        ctx.scroll_offset = 10;

        ctx.reset();

        assert!(ctx.plan_text.is_none());
        assert!(ctx.error.is_none());
        assert!(!ctx.is_analyze);
        assert_eq!(ctx.execution_time_ms, 0);
        assert_eq!(ctx.scroll_offset, 0);
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
}
