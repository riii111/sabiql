#[derive(Debug, Clone, PartialEq)]
pub struct ExplainPlan {
    pub raw_text: String,
    pub top_node_type: Option<String>,
    pub total_cost: Option<f64>,
    pub estimated_rows: Option<u64>,
    pub is_analyze: bool,
    pub execution_time_ms: u64,
}

impl ExplainPlan {
    pub fn execution_secs(&self) -> f64 {
        self.execution_time_ms as f64 / 1000.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonVerdict {
    Improved,
    Worsened,
    Similar,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonResult {
    pub verdict: ComparisonVerdict,
    pub reasons: Vec<String>,
}

const IMPROVED_THRESHOLD: f64 = 0.9;
const WORSENED_THRESHOLD: f64 = 1.1;
const MAX_REASONS: usize = 3;

fn parse_cost_fragment(line: &str) -> Option<(f64, u64)> {
    let cost_start = line.find("(cost=")?;
    let after_cost = line.get(cost_start + 6..)?;
    let dots = after_cost.find("..")?;
    let after_dots = after_cost.get(dots + 2..)?;

    let cost_end = after_dots.find(' ')?;
    let total_cost: f64 = after_dots.get(..cost_end)?.parse().ok()?;

    let rows_marker = after_dots.find("rows=")?;
    let after_rows = after_dots.get(rows_marker + 5..)?;
    let rows_end = after_rows
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_rows.len());
    let rows: u64 = after_rows.get(..rows_end)?.parse().ok()?;

    Some((total_cost, rows))
}

pub fn parse_explain_text(text: &str, is_analyze: bool, execution_time_ms: u64) -> ExplainPlan {
    let first_cost_line = text.lines().find(|line| line.contains("(cost="));

    let (top_node_type, total_cost, estimated_rows) = match first_cost_line {
        Some(line) => {
            let (cost, rows) = match parse_cost_fragment(line) {
                Some((c, r)) => (Some(c), Some(r)),
                None => (None, None),
            };

            let node_part = line.split("(cost=").next().unwrap_or("");
            let node_name = node_part.trim().trim_start_matches("->").trim().to_string();
            let node = if node_name.is_empty() {
                None
            } else {
                Some(node_name)
            };

            (node, cost, rows)
        }
        None => (None, None, None),
    };

    ExplainPlan {
        raw_text: text.to_string(),
        top_node_type,
        total_cost,
        estimated_rows,
        is_analyze,
        execution_time_ms,
    }
}

// ── Comparison ───────────────────────────────────────────────────────────────

pub fn compare_plans(baseline: &ExplainPlan, current: &ExplainPlan) -> ComparisonResult {
    let mut reasons: Vec<String> = Vec::new();

    let verdict = match (baseline.total_cost, current.total_cost) {
        (Some(b), Some(c)) => {
            let pct = if b > 0.0 {
                ((c - b) / b) * 100.0
            } else if c > 0.0 {
                100.0
            } else {
                0.0
            };

            let direction = if pct < 0.0 { "" } else { "+" };
            reasons.push(format!(
                "Total cost: {b:.2} \u{2192} {c:.2} ({direction}{pct:.1}%)"
            ));

            if c < b * IMPROVED_THRESHOLD {
                ComparisonVerdict::Improved
            } else if c > b * WORSENED_THRESHOLD {
                ComparisonVerdict::Worsened
            } else {
                ComparisonVerdict::Similar
            }
        }
        (None, None) => {
            reasons.push("Could not parse cost from either plan".to_string());
            ComparisonVerdict::Unavailable
        }
        _ => {
            reasons.push("Could not parse cost from one of the plans".to_string());
            ComparisonVerdict::Unavailable
        }
    };

    if verdict != ComparisonVerdict::Unavailable {
        if baseline.top_node_type != current.top_node_type {
            let b_node = baseline.top_node_type.as_deref().unwrap_or("(unknown)");
            let c_node = current.top_node_type.as_deref().unwrap_or("(unknown)");
            reasons.push(format!("{b_node} \u{2192} {c_node}"));
        }

        if let (Some(b_rows), Some(c_rows)) = (baseline.estimated_rows, current.estimated_rows)
            && b_rows != c_rows
        {
            reasons.push(format!("Estimated rows: {b_rows} \u{2192} {c_rows}"));
        }
    }

    reasons.truncate(MAX_REASONS);

    ComparisonResult { verdict, reasons }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    mod parse {
        use super::*;

        #[test]
        fn single_node_seq_scan() {
            let text = "Seq Scan on users  (cost=0.00..1000.00 rows=100 width=32)";
            let plan = parse_explain_text(text, false, 42);

            assert_eq!(plan.top_node_type.as_deref(), Some("Seq Scan on users"));
            assert_eq!(plan.total_cost, Some(1000.0));
            assert_eq!(plan.estimated_rows, Some(100));
            assert!(!plan.is_analyze);
            assert_eq!(plan.execution_time_ms, 42);
        }

        #[test]
        fn nested_plan_extracts_top_level_only() {
            let text = "\
Sort  (cost=0.00..1234.56 rows=100 width=32)
  Sort Key: id
  ->  Seq Scan on users  (cost=0.00..1000.00 rows=100 width=32)
        Filter: (active = true)";
            let plan = parse_explain_text(text, false, 0);

            assert_eq!(plan.top_node_type.as_deref(), Some("Sort"));
            assert_eq!(plan.total_cost, Some(1234.56));
            assert_eq!(plan.estimated_rows, Some(100));
        }

        #[test]
        fn explain_analyze_output() {
            let text = "\
Seq Scan on users  (cost=0.00..1000.00 rows=100 width=32) (actual time=0.010..0.500 rows=95 loops=1)
Planning Time: 0.050 ms
Execution Time: 0.600 ms";
            let plan = parse_explain_text(text, true, 1);

            assert_eq!(plan.total_cost, Some(1000.0));
            assert_eq!(plan.estimated_rows, Some(100));
            assert!(plan.is_analyze);
        }

        #[test]
        fn arrow_prefixed_node() {
            let text = "  ->  Index Scan using idx_users_email on users  (cost=0.28..8.30 rows=1 width=64)";
            let plan = parse_explain_text(text, false, 0);

            assert_eq!(
                plan.top_node_type.as_deref(),
                Some("Index Scan using idx_users_email on users")
            );
            assert_eq!(plan.total_cost, Some(8.30));
            assert_eq!(plan.estimated_rows, Some(1));
        }

        #[test]
        fn unparseable_text() {
            let text = "CREATE TABLE -- no cost info here";
            let plan = parse_explain_text(text, false, 0);

            assert!(plan.top_node_type.is_none());
            assert!(plan.total_cost.is_none());
            assert!(plan.estimated_rows.is_none());
        }

        #[test]
        fn empty_input() {
            let plan = parse_explain_text("", false, 0);

            assert!(plan.top_node_type.is_none());
            assert!(plan.total_cost.is_none());
            assert!(plan.estimated_rows.is_none());
        }

        #[test]
        fn whitespace_only_input() {
            let plan = parse_explain_text("   \n  \n  ", false, 0);

            assert!(plan.top_node_type.is_none());
            assert!(plan.total_cost.is_none());
        }
    }

    mod compare {
        use super::*;

        fn make_plan(cost: Option<f64>, rows: Option<u64>, node: Option<&str>) -> ExplainPlan {
            ExplainPlan {
                raw_text: String::new(),
                top_node_type: node.map(ToString::to_string),
                total_cost: cost,
                estimated_rows: rows,
                is_analyze: false,
                execution_time_ms: 0,
            }
        }

        #[test]
        fn improved_when_cost_drops_below_threshold() {
            let baseline = make_plan(Some(1000.0), Some(100), Some("Seq Scan"));
            let current = make_plan(Some(500.0), Some(100), Some("Seq Scan"));

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Improved);
        }

        #[test]
        fn worsened_when_cost_exceeds_threshold() {
            let baseline = make_plan(Some(100.0), Some(10), Some("Index Scan"));
            let current = make_plan(Some(1000.0), Some(10), Some("Seq Scan"));

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Worsened);
        }

        #[test]
        fn similar_within_threshold() {
            let baseline = make_plan(Some(100.0), Some(10), Some("Seq Scan"));
            let current = make_plan(Some(105.0), Some(10), Some("Seq Scan"));

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Similar);
        }

        #[test]
        fn boundary_at_exactly_0_9_is_similar() {
            let baseline = make_plan(Some(100.0), None, None);
            let current = make_plan(Some(90.0), None, None);

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Similar);
        }

        #[test]
        fn boundary_at_exactly_1_1_is_similar() {
            let baseline = make_plan(Some(100.0), None, None);
            let current = make_plan(Some(110.0), None, None);

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Similar);
        }

        #[test]
        fn both_costs_none() {
            let baseline = make_plan(None, None, None);
            let current = make_plan(None, None, None);

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Unavailable);
            assert!(
                result
                    .reasons
                    .iter()
                    .any(|r| r.contains("Could not parse cost"))
            );
        }

        #[test]
        fn one_cost_none() {
            let baseline = make_plan(Some(100.0), None, None);
            let current = make_plan(None, None, None);

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Unavailable);
            assert!(
                result
                    .reasons
                    .iter()
                    .any(|r| r.contains("Could not parse cost"))
            );
        }

        #[test]
        fn node_type_change_in_reasons() {
            let baseline = make_plan(Some(1000.0), Some(100), Some("Seq Scan"));
            let current = make_plan(Some(10.0), Some(1), Some("Index Scan"));

            let result = compare_plans(&baseline, &current);

            assert!(
                result
                    .reasons
                    .iter()
                    .any(|r| r.contains("Seq Scan") && r.contains("Index Scan"))
            );
        }

        #[test]
        fn same_node_type_not_in_reasons() {
            let baseline = make_plan(Some(100.0), Some(10), Some("Seq Scan"));
            let current = make_plan(Some(105.0), Some(10), Some("Seq Scan"));

            let result = compare_plans(&baseline, &current);

            assert!(
                !result
                    .reasons
                    .iter()
                    .any(|r| r.contains("Seq Scan \u{2192}"))
            );
        }

        #[test]
        fn row_estimate_change_in_reasons() {
            let baseline = make_plan(Some(100.0), Some(1000), Some("Seq Scan"));
            let current = make_plan(Some(105.0), Some(10), Some("Seq Scan"));

            let result = compare_plans(&baseline, &current);

            assert!(
                result
                    .reasons
                    .iter()
                    .any(|r| r.contains("Estimated rows: 1000 \u{2192} 10"))
            );
        }

        #[test]
        fn same_row_estimate_not_in_reasons() {
            let baseline = make_plan(Some(100.0), Some(10), Some("Seq Scan"));
            let current = make_plan(Some(105.0), Some(10), Some("Seq Scan"));

            let result = compare_plans(&baseline, &current);

            assert!(
                !result
                    .reasons
                    .iter()
                    .any(|r| r.starts_with("Estimated rows:"))
            );
        }

        #[test]
        fn reasons_capped_at_max() {
            let baseline = make_plan(Some(1000.0), Some(100), Some("Seq Scan"));
            let current = make_plan(Some(10.0), Some(1), Some("Index Scan"));

            let result = compare_plans(&baseline, &current);

            assert!(result.reasons.len() <= MAX_REASONS);
        }

        #[test]
        fn zero_baseline_cost_with_nonzero_current() {
            let baseline = make_plan(Some(0.0), None, None);
            let current = make_plan(Some(100.0), None, None);

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Worsened);
        }

        #[test]
        fn both_zero_cost() {
            let baseline = make_plan(Some(0.0), None, None);
            let current = make_plan(Some(0.0), None, None);

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Similar);
        }
    }
}
