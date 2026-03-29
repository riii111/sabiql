use crate::app::model::browse::json_tree::{JsonTree, LineType, TreeLine, TreeValue};

pub fn parse_json_tree(json_str: &str) -> Result<JsonTree, String> {
    let value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {e}"))?;

    let mut lines = Vec::new();
    build_lines(&value, 0, None, &mut lines);
    Ok(JsonTree::new(lines))
}

fn build_lines(
    value: &serde_json::Value,
    depth: usize,
    key: Option<String>,
    lines: &mut Vec<TreeLine>,
) {
    match value {
        serde_json::Value::Object(map) => {
            lines.push(TreeLine {
                depth,
                key,
                value: TreeValue::ObjectOpen {
                    child_count: map.len(),
                },
                collapsed: false,
                line_type: LineType::ObjectOpen,
            });
            for (k, v) in map {
                build_lines(v, depth + 1, Some(k.clone()), lines);
            }
            lines.push(TreeLine {
                depth,
                key: None,
                value: TreeValue::Closing,
                collapsed: false,
                line_type: LineType::ObjectClose,
            });
        }
        serde_json::Value::Array(arr) => {
            lines.push(TreeLine {
                depth,
                key,
                value: TreeValue::ArrayOpen {
                    child_count: arr.len(),
                },
                collapsed: false,
                line_type: LineType::ArrayOpen,
            });
            for item in arr {
                build_lines(item, depth + 1, None, lines);
            }
            lines.push(TreeLine {
                depth,
                key: None,
                value: TreeValue::Closing,
                collapsed: false,
                line_type: LineType::ArrayClose,
            });
        }
        serde_json::Value::Null => {
            let line_type = if key.is_some() {
                LineType::KeyValue
            } else {
                LineType::ArrayItem
            };
            lines.push(TreeLine {
                depth,
                key,
                value: TreeValue::Null,
                collapsed: false,
                line_type,
            });
        }
        serde_json::Value::Bool(b) => {
            let line_type = if key.is_some() {
                LineType::KeyValue
            } else {
                LineType::ArrayItem
            };
            lines.push(TreeLine {
                depth,
                key,
                value: TreeValue::Bool(*b),
                collapsed: false,
                line_type,
            });
        }
        serde_json::Value::Number(n) => {
            let line_type = if key.is_some() {
                LineType::KeyValue
            } else {
                LineType::ArrayItem
            };
            lines.push(TreeLine {
                depth,
                key,
                value: TreeValue::Number(n.to_string()),
                collapsed: false,
                line_type,
            });
        }
        serde_json::Value::String(s) => {
            let line_type = if key.is_some() {
                LineType::KeyValue
            } else {
                LineType::ArrayItem
            };
            lines.push(TreeLine {
                depth,
                key,
                value: TreeValue::String(s.clone()),
                collapsed: false,
                line_type,
            });
        }
    }
}

pub fn visible_line_indices(tree: &JsonTree) -> Vec<usize> {
    let lines = tree.lines();
    let mut result = Vec::with_capacity(lines.len());
    let mut skip_depth: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        if let Some(sd) = skip_depth {
            if line.depth <= sd
                && matches!(line.line_type, LineType::ObjectClose | LineType::ArrayClose)
            {
                // This is the closing brace for the collapsed node — skip it too
                if line.depth == sd {
                    skip_depth = None;
                }
                continue;
            }
            if line.depth > sd {
                continue;
            }
            // We've exited the collapsed subtree
            skip_depth = None;
        }

        result.push(i);

        if line.collapsed && matches!(line.line_type, LineType::ObjectOpen | LineType::ArrayOpen) {
            skip_depth = Some(line.depth);
        }
    }

    result
}

pub fn find_matches(tree: &JsonTree, visible_indices: &[usize], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    let lines = tree.lines();

    let matches_bool = query_lower == "true" || query_lower == "false";
    let matches_null = "null".contains(&query_lower);

    visible_indices
        .iter()
        .copied()
        .filter(|&i| {
            let line = &lines[i];
            if let Some(key) = &line.key
                && key.to_lowercase().contains(&query_lower)
            {
                return true;
            }
            match &line.value {
                TreeValue::String(s) => s.to_lowercase().contains(&query_lower),
                TreeValue::Number(n) => n.contains(&query_lower),
                TreeValue::Bool(b) => {
                    matches_bool && (if *b { "true" } else { "false" }).contains(&query_lower)
                }
                TreeValue::Null => matches_null,
                _ => false,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_json_tree_tests {
        use super::*;

        #[test]
        fn empty_object_returns_open_close() {
            let tree = parse_json_tree("{}").unwrap();

            assert_eq!(tree.line_count(), 2);
            assert!(matches!(tree.lines()[0].line_type, LineType::ObjectOpen));
            assert!(matches!(tree.lines()[1].line_type, LineType::ObjectClose));
        }

        #[test]
        fn empty_array_returns_open_close() {
            let tree = parse_json_tree("[]").unwrap();

            assert_eq!(tree.line_count(), 2);
            assert!(matches!(tree.lines()[0].line_type, LineType::ArrayOpen));
            assert!(matches!(tree.lines()[1].line_type, LineType::ArrayClose));
        }

        #[test]
        fn scalar_null_returns_single_line() {
            let tree = parse_json_tree("null").unwrap();

            assert_eq!(tree.line_count(), 1);
            assert!(matches!(tree.lines()[0].value, TreeValue::Null));
        }

        #[test]
        fn scalar_bool_returns_single_line() {
            let tree = parse_json_tree("true").unwrap();

            assert_eq!(tree.line_count(), 1);
            assert!(matches!(tree.lines()[0].value, TreeValue::Bool(true)));
        }

        #[test]
        fn scalar_number_returns_single_line() {
            let tree = parse_json_tree("42").unwrap();

            assert_eq!(tree.line_count(), 1);
            assert!(matches!(tree.lines()[0].value, TreeValue::Number(ref n) if n == "42"));
        }

        #[test]
        fn scalar_string_returns_single_line() {
            let tree = parse_json_tree("\"hello\"").unwrap();

            assert_eq!(tree.line_count(), 1);
            assert!(matches!(tree.lines()[0].value, TreeValue::String(ref s) if s == "hello"));
        }

        #[test]
        fn nested_object_returns_correct_depth() {
            let tree = parse_json_tree(r#"{"a": {"b": 1}}"#).unwrap();

            // { (depth 0), "a": { (depth 1), "b": 1 (depth 2), } (depth 1), } (depth 0)
            assert_eq!(tree.line_count(), 5);
            assert_eq!(tree.lines()[0].depth, 0);
            assert_eq!(tree.lines()[1].depth, 1);
            assert_eq!(tree.lines()[2].depth, 2);
            assert_eq!(tree.lines()[3].depth, 1);
            assert_eq!(tree.lines()[4].depth, 0);
        }

        #[test]
        fn array_items_returns_array_item_type() {
            let tree = parse_json_tree("[1, 2, 3]").unwrap();

            // [ (depth 0), 1 (depth 1), 2 (depth 1), 3 (depth 1), ] (depth 0)
            assert_eq!(tree.line_count(), 5);
            for line in &tree.lines()[1..4] {
                assert!(matches!(line.line_type, LineType::ArrayItem));
            }
        }

        #[test]
        fn key_value_pairs_returns_keys() {
            let tree = parse_json_tree(r#"{"name": "test", "count": 5}"#).unwrap();

            let kv_lines: Vec<_> = tree
                .lines()
                .iter()
                .filter(|l| matches!(l.line_type, LineType::KeyValue))
                .collect();
            assert_eq!(kv_lines.len(), 2);
            let mut keys: Vec<_> = kv_lines.iter().filter_map(|l| l.key.as_deref()).collect();
            keys.sort_unstable();
            assert_eq!(keys, vec!["count", "name"]);
        }

        #[test]
        fn object_open_returns_correct_child_count() {
            let tree = parse_json_tree(r#"{"a": 1, "b": 2, "c": 3}"#).unwrap();

            assert!(matches!(
                tree.lines()[0].value,
                TreeValue::ObjectOpen { child_count: 3 }
            ));
        }

        #[test]
        fn malformed_json_returns_error() {
            let result = parse_json_tree("{invalid}");

            assert!(result.is_err());
        }

        #[test]
        fn empty_string_returns_error() {
            let result = parse_json_tree("");

            assert!(result.is_err());
        }

        #[test]
        fn deeply_nested_json_returns_correct_depth() {
            let json = r#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":{"j":{"k":{"l":{"m":{"n":{"o":{"p":{"q":{"r":{"s":{"t":1}}}}}}}}}}}}}}}}}}}}"#;
            let tree = parse_json_tree(json).unwrap();

            // 20 levels of nesting: 20 open + 20 close + 1 leaf = 41 lines
            assert_eq!(tree.line_count(), 41);
            // The deepest leaf should be at depth 20
            let max_depth = tree.lines().iter().map(|l| l.depth).max().unwrap();
            assert_eq!(max_depth, 20);
        }
    }

    mod visible_line_indices_tests {
        use super::*;

        #[test]
        fn uncollapsed_tree_returns_all_lines() {
            let tree = parse_json_tree(r#"{"a": 1, "b": 2}"#).unwrap();

            let visible = visible_line_indices(&tree);

            // { "a": 1, "b": 2, }
            assert_eq!(visible.len(), tree.line_count());
        }

        #[test]
        fn collapsed_object_returns_visible_lines_without_children() {
            let mut tree = parse_json_tree(r#"{"a": {"b": 1}, "c": 2}"#).unwrap();
            // Collapse the inner object {"b": 1} at index 1
            tree.toggle_fold(1);

            let visible = visible_line_indices(&tree);

            // Visible: { (0), "a": { (1, collapsed), "c": 2 (4), } (5)
            // Hidden: "b": 1 (2), } (3)
            assert_eq!(visible.len(), 4);
            assert!(!visible.contains(&2));
            assert!(!visible.contains(&3));
        }

        #[test]
        fn collapsed_root_returns_only_root_line() {
            let mut tree = parse_json_tree(r#"{"a": 1, "b": 2}"#).unwrap();
            tree.toggle_fold(0);

            let visible = visible_line_indices(&tree);

            // Only the root { is visible
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0], 0);
        }

        #[test]
        fn collapsed_array_returns_only_opener() {
            let mut tree = parse_json_tree(r"[1, 2, 3]").unwrap();
            tree.toggle_fold(0);

            let visible = visible_line_indices(&tree);

            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0], 0);
        }

        #[test]
        fn empty_tree_returns_empty_indices() {
            let tree = JsonTree::default();

            let visible = visible_line_indices(&tree);

            assert!(visible.is_empty());
        }
    }

    mod find_matches_tests {
        use super::*;

        #[test]
        fn key_search_returns_case_insensitive_match() {
            let tree = parse_json_tree(r#"{"Email": "test@example.com"}"#).unwrap();
            let visible = visible_line_indices(&tree);

            let matches = find_matches(&tree, &visible, "email");

            assert_eq!(matches.len(), 1);
        }

        #[test]
        fn string_value_search_returns_match() {
            let tree = parse_json_tree(r#"{"name": "Alice", "city": "Berlin"}"#).unwrap();
            let visible = visible_line_indices(&tree);

            let matches = find_matches(&tree, &visible, "alice");

            assert_eq!(matches.len(), 1);
        }

        #[test]
        fn empty_query_returns_no_matches() {
            let tree = parse_json_tree(r#"{"a": 1}"#).unwrap();
            let visible = visible_line_indices(&tree);

            let matches = find_matches(&tree, &visible, "");

            assert!(matches.is_empty());
        }

        #[test]
        fn collapsed_children_returns_no_matches() {
            let mut tree = parse_json_tree(r#"{"a": {"hidden": "secret"}, "b": 1}"#).unwrap();
            tree.toggle_fold(1);
            let visible = visible_line_indices(&tree);

            let matches = find_matches(&tree, &visible, "hidden");

            assert!(matches.is_empty());
        }

        #[test]
        fn number_value_search_returns_match() {
            let tree = parse_json_tree(r#"{"count": 42}"#).unwrap();
            let visible = visible_line_indices(&tree);

            let matches = find_matches(&tree, &visible, "42");

            assert_eq!(matches.len(), 1);
        }
    }
}
