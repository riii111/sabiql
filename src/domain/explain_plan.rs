use std::collections::HashMap;

use crate::query_result::QueryResult;

#[derive(Debug, Clone, PartialEq)]
pub struct ExplainPlan {
    pub raw_text: String,
    pub top_node_type: Option<String>,
    pub total_cost: Option<f64>,
    pub estimated_rows: Option<f64>,
    pub actual_start_ms: Option<f64>,
    pub actual_end_ms: Option<f64>,
    pub actual_rows: Option<f64>,
    pub loops: Option<u64>,
    pub is_analyze: bool,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone)]
struct SqliteExplainPlanRow {
    id: i64,
    parent: i64,
    detail: String,
}

fn column_index(columns: &[String], name: &str) -> Option<usize> {
    columns
        .iter()
        .position(|column| column.eq_ignore_ascii_case(name))
}

fn sqlite_explain_plan_rows(result: &QueryResult) -> Option<Vec<SqliteExplainPlanRow>> {
    let id_index = column_index(&result.columns, "id")?;
    let parent_index = column_index(&result.columns, "parent")?;
    let detail_index = column_index(&result.columns, "detail")?;

    (0..result.data_row_count())
        .map(|row_idx| {
            Some(SqliteExplainPlanRow {
                id: result.display_value_at(row_idx, id_index)?.parse().ok()?,
                parent: result
                    .display_value_at(row_idx, parent_index)?
                    .parse()
                    .ok()?,
                detail: result.display_value_at(row_idx, detail_index)?,
            })
        })
        .collect()
}

fn sqlite_explain_plan_depth(
    row: &SqliteExplainPlanRow,
    parents_by_id: &HashMap<i64, i64>,
    max_depth: usize,
) -> usize {
    let mut depth = 0usize;
    let mut parent = row.parent;
    while parents_by_id.contains_key(&parent) && depth < max_depth {
        depth += 1;
        parent = parents_by_id[&parent];
    }
    depth
}

fn sqlite_explain_plan_text(result: &QueryResult) -> Option<String> {
    let rows = sqlite_explain_plan_rows(result)?;
    let parents_by_id = rows
        .iter()
        .map(|row| (row.id, row.parent))
        .collect::<HashMap<_, _>>();
    let max_depth = rows.len();
    Some(
        rows.iter()
            .map(|row| {
                let depth = sqlite_explain_plan_depth(row, &parents_by_id, max_depth);
                if depth == 0 {
                    row.detail.clone()
                } else {
                    format!("{}- {}", "  ".repeat(depth), row.detail)
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn sqlite_explain_query_plan_text_from_result(result: &QueryResult) -> String {
    sqlite_explain_plan_text(result).unwrap_or_else(|| {
        (0..result.data_row_count())
            .filter_map(|row_idx| result.display_value_at(row_idx, 0))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn first_result_column_text(result: &QueryResult) -> String {
    (0..result.data_row_count())
        .filter_map(|row_idx| result.display_value_at(row_idx, 0))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn postgres_explain_plan_text_from_result(result: &QueryResult) -> String {
    first_result_column_text(result)
}

pub fn mysql_explain_plan_text_from_result(result: &QueryResult) -> String {
    first_result_column_text(result)
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

fn parse_cost_fragment(line: &str) -> Option<(f64, f64)> {
    let cost_start = line.find("(cost=")?;
    let after_cost = line.get(cost_start + 6..)?;
    let dots = after_cost.find("..")?;
    let after_dots = after_cost.get(dots + 2..)?;

    let cost_end = after_dots.find(' ')?;
    let total_cost: f64 = after_dots.get(..cost_end)?.parse().ok()?;

    let rows_marker = after_dots.find("rows=")?;
    let after_rows = after_dots.get(rows_marker + 5..)?;
    let rows_end = after_rows
        .find(|c: char| c.is_whitespace() || c == ')')
        .unwrap_or(after_rows.len());
    let rows: f64 = after_rows.get(..rows_end)?.parse().ok()?;

    Some((total_cost, rows))
}

fn parse_mysql_cost_fragment(line: &str) -> (Option<f64>, Option<f64>) {
    let Some(cost_start) = line.find("(cost=") else {
        return (None, None);
    };
    let Some(after_cost) = line.get(cost_start + 6..) else {
        return (None, None);
    };
    let cost_end = after_cost
        .find(|c: char| c.is_whitespace() || c == ')')
        .unwrap_or(after_cost.len());
    let cost_token = after_cost.get(..cost_end).unwrap_or_default();
    let total_cost = cost_token
        .rsplit_once("..")
        .map_or(cost_token, |(_, total)| total)
        .parse()
        .ok();

    let estimated_rows = line.find("rows=").and_then(|rows_start| {
        let after_rows = line.get(rows_start + 5..)?;
        let rows_end = after_rows
            .find(|c: char| c.is_whitespace() || c == ')')
            .unwrap_or(after_rows.len());
        after_rows.get(..rows_end)?.parse().ok()
    });

    (total_cost, estimated_rows)
}

fn mysql_node_name(line: &str) -> Option<String> {
    let node_name = line
        .split("(cost=")
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches("->")
        .trim();
    (!node_name.is_empty()).then(|| node_name.to_string())
}

fn parse_finite_f64(token: &str) -> Option<f64> {
    let value: f64 = token.parse().ok()?;
    value.is_finite().then_some(value)
}

fn parse_mysql_loops(token: &str) -> Option<u64> {
    let token = token.trim();
    let (mantissa, exponent) = token
        .split_once(['e', 'E'])
        .map_or((token, 0i64), |(mantissa, exponent)| {
            (mantissa, exponent.parse().unwrap_or(i64::MIN))
        });
    if exponent == i64::MIN || mantissa.is_empty() {
        return None;
    }

    let mantissa = mantissa.strip_prefix('+').unwrap_or(mantissa);
    if mantissa.starts_with('-') || mantissa.is_empty() {
        return None;
    }
    let mut parts = mantissa.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || whole.is_empty() && fraction.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    let digits = format!("{whole}{fraction}");
    let decimal_places = fraction.len() as i64 - exponent;
    let significant = if decimal_places > 0 {
        let decimal_places = usize::try_from(decimal_places).ok()?;
        if decimal_places > digits.len()
            || !digits[digits.len() - decimal_places..]
                .bytes()
                .all(|byte| byte == b'0')
        {
            return None;
        }
        &digits[..digits.len() - decimal_places]
    } else {
        &digits
    };

    let trailing_zeroes = usize::try_from(decimal_places.saturating_neg()).ok()?;
    let significant = significant.trim_start_matches('0');
    if significant.is_empty() {
        return Some(0);
    }
    if significant.len().saturating_add(trailing_zeroes) > 20 {
        return None;
    }

    let mut integer = significant.to_string();
    integer.push_str(&"0".repeat(trailing_zeroes));
    if integer.len() == 20 && integer.as_str() > u64::MAX.to_string().as_str() {
        return None;
    }
    integer.parse().ok()
}

fn parse_mysql_actual_fragment(line: &str) -> (Option<f64>, Option<f64>, Option<f64>, Option<u64>) {
    let Some(start) = line.find("(actual time=") else {
        return (None, None, None, None);
    };
    let fragment = line.get(start + 1..).unwrap_or_default();
    let mut actual_start_ms = None;
    let mut actual_end_ms = None;
    let mut actual_rows = None;
    let mut loops = None;

    for token in fragment.split_whitespace() {
        if let Some(time) = token.strip_prefix("time=") {
            if let Some((start, end)) = time.split_once("..") {
                actual_start_ms = parse_finite_f64(start);
                actual_end_ms = parse_finite_f64(end.trim_end_matches(')'));
            }
        } else if let Some(rows) = token.strip_prefix("rows=") {
            actual_rows = parse_finite_f64(rows.trim_end_matches(')'));
        } else if let Some(value) = token.strip_prefix("loops=") {
            loops = parse_mysql_loops(value.trim_end_matches(')'));
        }
    }

    (actual_start_ms, actual_end_ms, actual_rows, loops)
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
        actual_start_ms: None,
        actual_end_ms: None,
        actual_rows: None,
        loops: None,
        is_analyze,
        execution_time_ms,
    }
}

pub fn parse_mysql_tree_explain_text(
    text: &str,
    is_analyze: bool,
    execution_time_ms: u64,
) -> ExplainPlan {
    let first_cost_line = text.lines().find(|line| line.contains("(cost="));
    let (
        top_node_type,
        total_cost,
        estimated_rows,
        actual_start_ms,
        actual_end_ms,
        actual_rows,
        loops,
    ) = first_cost_line.map_or((None, None, None, None, None, None, None), |line| {
        let (cost, rows) = parse_mysql_cost_fragment(line);
        let (actual_start_ms, actual_end_ms, actual_rows, loops) =
            parse_mysql_actual_fragment(line);
        (
            mysql_node_name(line),
            cost,
            rows,
            actual_start_ms,
            actual_end_ms,
            actual_rows,
            loops,
        )
    });

    ExplainPlan {
        raw_text: text.to_string(),
        top_node_type,
        total_cost,
        estimated_rows,
        actual_start_ms,
        actual_end_ms,
        actual_rows,
        loops,
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
            && b_rows.partial_cmp(&c_rows) != Some(std::cmp::Ordering::Equal)
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
    use crate::query_result::QuerySource;

    mod parse {
        use super::*;

        #[test]
        fn single_node_seq_scan() {
            let text = "Seq Scan on users  (cost=0.00..1000.00 rows=100 width=32)";
            let plan = parse_explain_text(text, false, 42);

            assert_eq!(plan.top_node_type.as_deref(), Some("Seq Scan on users"));
            assert_eq!(plan.total_cost, Some(1000.0));
            assert_eq!(plan.estimated_rows, Some(100.0));
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
            assert_eq!(plan.estimated_rows, Some(100.0));
        }

        #[test]
        fn explain_analyze_output() {
            let text = "\
Seq Scan on users  (cost=0.00..1000.00 rows=100 width=32) (actual time=0.010..0.500 rows=95 loops=1)
Planning Time: 0.050 ms
Execution Time: 0.600 ms";
            let plan = parse_explain_text(text, true, 1);

            assert_eq!(plan.total_cost, Some(1000.0));
            assert_eq!(plan.estimated_rows, Some(100.0));
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
            assert_eq!(plan.estimated_rows, Some(1.0));
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

        #[test]
        fn mysql_tree_parses_decimal_and_scientific_values() {
            let text = "-> Filter: (id > 10)  (cost=1.25e-1..2.5 rows=3.75e+2)\n    -> Table scan on users  (cost=0.1 rows=1)";
            let plan = parse_mysql_tree_explain_text(text, false, 7);

            assert_eq!(plan.top_node_type.as_deref(), Some("Filter: (id > 10)"));
            assert_eq!(plan.total_cost, Some(2.5));
            assert_eq!(plan.estimated_rows, Some(375.0));
            assert_eq!(plan.raw_text, text);
        }

        #[test]
        fn mysql_tree_supports_single_cost_value() {
            let plan = parse_mysql_tree_explain_text(
                "-> Table scan on users  (cost=1.25 rows=2.5)",
                false,
                0,
            );

            assert_eq!(plan.total_cost, Some(1.25));
            assert_eq!(plan.estimated_rows, Some(2.5));
        }

        #[test]
        fn mysql_tree_keeps_raw_text_when_node_metrics_are_unavailable() {
            let text = "-> Filter: (id > 10)  (cost=unknown rows=unknown)";
            let plan = parse_mysql_tree_explain_text(text, false, 0);

            assert_eq!(plan.raw_text, text);
            assert_eq!(plan.top_node_type.as_deref(), Some("Filter: (id > 10)"));
            assert!(plan.total_cost.is_none());
            assert!(plan.estimated_rows.is_none());
        }

        #[test]
        fn mysql_tree_parses_top_level_actual_metrics() {
            let text = "-> Table scan on users  (cost=1.25 rows=2.5) (actual time=0.010..0.500 rows=95 loops=1)";
            let plan = parse_mysql_tree_explain_text(text, true, 7);

            assert_eq!(plan.actual_start_ms, Some(0.010));
            assert_eq!(plan.actual_end_ms, Some(0.500));
            assert_eq!(plan.actual_rows, Some(95.0));
            assert_eq!(plan.loops, Some(1));
        }

        #[test]
        fn mysql_tree_parses_scientific_loop_count() {
            let plan = parse_mysql_tree_explain_text(
                "-> Table scan on users  (cost=1 rows=1) (actual time=0..1 rows=1 loops=1e+6)",
                true,
                0,
            );

            assert_eq!(plan.loops, Some(1_000_000));
        }

        #[test]
        fn mysql_tree_rejects_invalid_loop_counts_but_keeps_raw_text() {
            for loops in ["1.5", "-1", "NaN", "inf", "1e309", "18446744073709551616"] {
                let text = format!(
                    "-> Table scan on users  (cost=1 rows=1) (actual time=0..1 rows=1 loops={loops})"
                );
                let plan = parse_mysql_tree_explain_text(&text, true, 0);

                assert_eq!(plan.loops, None, "{loops}");
                assert_eq!(plan.raw_text, text, "{loops}");
            }
        }

        #[test]
        fn mysql_tree_marks_missing_actual_metrics_unavailable() {
            let plan =
                parse_mysql_tree_explain_text("-> Table scan on users  (cost=1 rows=1)", true, 0);

            assert_eq!(plan.actual_start_ms, None);
            assert_eq!(plan.actual_end_ms, None);
            assert_eq!(plan.actual_rows, None);
            assert_eq!(plan.loops, None);
        }
    }

    #[test]
    fn mysql_plan_text_uses_the_first_result_column_without_sqlite_tree_reconstruction() {
        let result = QueryResult::success(
            "EXPLAIN FORMAT=TREE SELECT 1".to_string(),
            vec!["EXPLAIN".to_string(), "ignored".to_string()],
            vec![
                vec!["-> first".to_string(), "x".to_string()],
                vec!["  -> second".to_string(), "y".to_string()],
            ],
            0,
            QuerySource::Adhoc,
        );

        assert_eq!(
            mysql_explain_plan_text_from_result(&result),
            "-> first\n  -> second"
        );
    }

    mod compare {
        use super::*;

        fn make_plan(cost: Option<f64>, rows: Option<f64>, node: Option<&str>) -> ExplainPlan {
            ExplainPlan {
                raw_text: String::new(),
                top_node_type: node.map(ToString::to_string),
                total_cost: cost,
                estimated_rows: rows,
                actual_start_ms: None,
                actual_end_ms: None,
                actual_rows: None,
                loops: None,
                is_analyze: false,
                execution_time_ms: 0,
            }
        }

        #[test]
        fn improved_when_cost_drops_below_threshold() {
            let baseline = make_plan(Some(1000.0), Some(100.0), Some("Seq Scan"));
            let current = make_plan(Some(500.0), Some(100.0), Some("Seq Scan"));

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Improved);
        }

        #[test]
        fn worsened_when_cost_exceeds_threshold() {
            let baseline = make_plan(Some(100.0), Some(10.0), Some("Index Scan"));
            let current = make_plan(Some(1000.0), Some(10.0), Some("Seq Scan"));

            let result = compare_plans(&baseline, &current);

            assert_eq!(result.verdict, ComparisonVerdict::Worsened);
        }

        #[test]
        fn similar_within_threshold() {
            let baseline = make_plan(Some(100.0), Some(10.0), Some("Seq Scan"));
            let current = make_plan(Some(105.0), Some(10.0), Some("Seq Scan"));

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
            let baseline = make_plan(Some(1000.0), Some(100.0), Some("Seq Scan"));
            let current = make_plan(Some(10.0), Some(1.0), Some("Index Scan"));

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
            let baseline = make_plan(Some(100.0), Some(10.0), Some("Seq Scan"));
            let current = make_plan(Some(105.0), Some(10.0), Some("Seq Scan"));

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
            let baseline = make_plan(Some(100.0), Some(1000.0), Some("Seq Scan"));
            let current = make_plan(Some(105.0), Some(10.0), Some("Seq Scan"));

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
            let baseline = make_plan(Some(100.0), Some(10.0), Some("Seq Scan"));
            let current = make_plan(Some(105.0), Some(10.0), Some("Seq Scan"));

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
            let baseline = make_plan(Some(1000.0), Some(100.0), Some("Seq Scan"));
            let current = make_plan(Some(10.0), Some(1.0), Some("Index Scan"));

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
