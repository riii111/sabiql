#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonDiffLine {
    Context(String),
    Added(String),
    Removed(String),
    Ellipsis,
}

/// Compute a structured diff between two JSON strings.
///
/// Returns `None` if either value is not valid JSON, or if both are
/// identical after pretty-printing. `context_lines` controls how many
/// unchanged lines surround each change hunk.
pub fn compute_json_diff(
    before: &str,
    after: &str,
    context_lines: usize,
) -> Option<Vec<JsonDiffLine>> {
    let before_val: serde_json::Value = serde_json::from_str(before).ok()?;
    let after_val: serde_json::Value = serde_json::from_str(after).ok()?;

    let before_pretty = serde_json::to_string_pretty(&before_val).ok()?;
    let after_pretty = serde_json::to_string_pretty(&after_val).ok()?;

    if before_pretty == after_pretty {
        return None;
    }

    let before_lines: Vec<&str> = before_pretty.lines().collect();
    let after_lines: Vec<&str> = after_pretty.lines().collect();

    // Guard against O(n*m) blowup on very large JSON values.
    const MAX_LINES: usize = 500;
    if before_lines.len() > MAX_LINES || after_lines.len() > MAX_LINES {
        return None;
    }

    let tagged = lcs_diff(&before_lines, &after_lines);
    Some(collapse_context(&tagged, context_lines))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffTag {
    Equal(String),
    Insert(String),
    Delete(String),
}

/// Classic LCS-based diff producing a sequence of Equal/Insert/Delete tags.
fn lcs_diff(old: &[&str], new: &[&str]) -> Vec<DiffTag> {
    let n = old.len();
    let m = new.len();

    // Build LCS table
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            table[i][j] = if old[i - 1] == new[j - 1] {
                table[i - 1][j - 1] + 1
            } else {
                table[i - 1][j].max(table[i][j - 1])
            };
        }
    }

    // Back-track to produce diff
    let mut result = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            result.push(DiffTag::Equal(old[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
            result.push(DiffTag::Insert(new[j - 1].to_string()));
            j -= 1;
        } else {
            result.push(DiffTag::Delete(old[i - 1].to_string()));
            i -= 1;
        }
    }
    result.reverse();
    result
}

/// Collapse Equal runs into Context + Ellipsis based on `context_lines`.
fn collapse_context(tags: &[DiffTag], context_lines: usize) -> Vec<JsonDiffLine> {
    // First, identify which Equal lines are within `context_lines` of a change.
    let len = tags.len();
    let mut keep = vec![false; len];

    // Mark lines near changes
    let mut last_change: Option<usize> = None;
    for (i, tag) in tags.iter().enumerate() {
        match tag {
            DiffTag::Insert(_) | DiffTag::Delete(_) => {
                keep[i] = true;
                // Mark trailing context from previous change is already handled.
                // Mark leading context for this change.
                for k in (0..i).rev().take(context_lines) {
                    if matches!(tags[k], DiffTag::Equal(_)) {
                        keep[k] = true;
                    } else {
                        break;
                    }
                }
                last_change = Some(i);
            }
            DiffTag::Equal(_) => {
                if let Some(lc) = last_change
                    && i - lc <= context_lines
                {
                    keep[i] = true;
                }
            }
        }
    }

    // Build output, inserting Ellipsis for skipped Equal runs.
    let mut output = Vec::new();
    let mut in_ellipsis = false;

    for (i, tag) in tags.iter().enumerate() {
        if keep[i] {
            in_ellipsis = false;
            match tag {
                DiffTag::Equal(s) => output.push(JsonDiffLine::Context(s.clone())),
                DiffTag::Insert(s) => output.push(JsonDiffLine::Added(s.clone())),
                DiffTag::Delete(s) => output.push(JsonDiffLine::Removed(s.clone())),
            }
        } else if !in_ellipsis {
            output.push(JsonDiffLine::Ellipsis);
            in_ellipsis = true;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_json_returns_none() {
        let result = compute_json_diff("not json", "also not json", 1);

        assert_eq!(result, None);
    }

    #[test]
    fn identical_json_returns_none() {
        let before = r#"{"b": 2, "a": 1}"#;
        let after = r#"{"a": 1, "b": 2}"#;

        let result = compute_json_diff(before, after, 1);

        assert_eq!(result, None);
    }

    #[test]
    fn single_value_change_returns_diff_with_context() {
        let before = r#"{"name": "Alice", "age": 30}"#;
        let after = r#"{"name": "Alice", "age": 31}"#;

        let result = compute_json_diff(before, after, 1).unwrap();

        // serde_json sorts alphabetically: "age" first, then "name"
        assert!(
            result
                .iter()
                .any(|l| matches!(l, JsonDiffLine::Removed(s) if s.contains("30")))
        );
        assert!(
            result
                .iter()
                .any(|l| matches!(l, JsonDiffLine::Added(s) if s.contains("31")))
        );
        assert!(
            result
                .iter()
                .any(|l| matches!(l, JsonDiffLine::Context(s) if s.contains("\"name\"")))
        );
    }

    #[test]
    fn nested_change_collapses_unchanged() {
        let before = r#"{
            "a": 1, "b": 2, "c": 3, "d": 4,
            "nested": {"x": 10, "y": 20, "z": 30}
        }"#;
        let after = r#"{
            "a": 1, "b": 2, "c": 3, "d": 4,
            "nested": {"x": 10, "y": 99, "z": 30}
        }"#;

        let result = compute_json_diff(before, after, 1).unwrap();

        assert!(
            result.contains(&JsonDiffLine::Ellipsis),
            "should collapse unchanged top-level keys"
        );
        assert!(
            result.contains(&JsonDiffLine::Removed("    \"y\": 20,".to_string()))
                || result.contains(&JsonDiffLine::Removed("    \"y\": 20".to_string()))
        );
    }

    #[test]
    fn array_element_added_returns_added_lines() {
        let before = r#"{"items": ["a", "b"]}"#;
        let after = r#"{"items": ["a", "b", "c"]}"#;

        let result = compute_json_diff(before, after, 1).unwrap();

        assert!(
            result
                .iter()
                .any(|l| matches!(l, JsonDiffLine::Added(s) if s.contains("\"c\"")))
        );
    }

    #[test]
    fn array_element_removed_returns_removed_lines() {
        let before = r#"{"items": ["a", "b", "c"]}"#;
        let after = r#"{"items": ["a", "b"]}"#;

        let result = compute_json_diff(before, after, 1).unwrap();

        assert!(
            result
                .iter()
                .any(|l| matches!(l, JsonDiffLine::Removed(s) if s.contains("\"c\"")))
        );
    }

    #[test]
    fn context_zero_returns_only_changes_and_ellipsis() {
        let before = r#"{"a": 1, "b": 2, "c": 3}"#;
        let after = r#"{"a": 1, "b": 99, "c": 3}"#;

        let result = compute_json_diff(before, after, 0).unwrap();

        for line in &result {
            assert!(
                !matches!(line, JsonDiffLine::Context(_)),
                "context=0 should have no Context lines, found: {line:?}"
            );
        }
        assert!(result.iter().any(|l| matches!(l, JsonDiffLine::Removed(_))));
        assert!(result.iter().any(|l| matches!(l, JsonDiffLine::Added(_))));
    }

    #[test]
    fn multiple_hunks_merge_when_close() {
        // "a" and "c" change, with only "b" between them.
        // With context=1, the gap (1 line) <= 2*context(2), so no Ellipsis between them.
        let before = r#"{"a": 1, "b": 2, "c": 3}"#;
        let after = r#"{"a": 10, "b": 2, "c": 30}"#;

        let result = compute_json_diff(before, after, 1).unwrap();

        let change_count = result
            .iter()
            .filter(|l| matches!(l, JsonDiffLine::Added(_) | JsonDiffLine::Removed(_)))
            .count();
        assert!(change_count >= 4, "should have changes for a and c");

        // The "b" line between changes should be Context, not Ellipsis
        let between_ellipsis = result.windows(3).any(|w| {
            matches!(w[0], JsonDiffLine::Added(_) | JsonDiffLine::Removed(_))
                && matches!(w[1], JsonDiffLine::Ellipsis)
                && matches!(w[2], JsonDiffLine::Added(_) | JsonDiffLine::Removed(_))
        });
        assert!(
            !between_ellipsis,
            "close hunks should merge without Ellipsis between them"
        );
    }

    #[test]
    fn change_at_start_returns_no_leading_ellipsis() {
        let before = r#"{"a": 1, "b": 2}"#;
        let after = r#"{"a": 99, "b": 2}"#;

        let result = compute_json_diff(before, after, 1).unwrap();

        assert!(
            !matches!(result.first(), Some(JsonDiffLine::Ellipsis)),
            "first line should not be Ellipsis when change is at start"
        );
    }

    #[test]
    fn change_at_end_returns_no_trailing_ellipsis() {
        let before = r#"{"a": 1, "b": 2}"#;
        let after = r#"{"a": 1, "b": 99}"#;

        let result = compute_json_diff(before, after, 1).unwrap();

        assert!(
            !matches!(result.last(), Some(JsonDiffLine::Ellipsis)),
            "last line should not be Ellipsis when change is at end"
        );
    }
}
