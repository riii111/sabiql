use std::collections::HashMap;

use crate::domain::{CommandTag, QueryResult, QuerySource};

use super::lexer::{
    dml_keyword, first_keyword, is_rollback_to, rollback_to_target, savepoint_target,
    second_keyword,
};

fn ddl_tag(query: &str) -> Option<CommandTag> {
    let object = second_keyword(query)
        .unwrap_or("OBJECT")
        .to_ascii_uppercase();
    let keyword = first_keyword(query);
    if keyword.eq_ignore_ascii_case("CREATE") {
        Some(CommandTag::Create(object))
    } else if keyword.eq_ignore_ascii_case("DROP") {
        Some(CommandTag::Drop(object))
    } else if keyword.eq_ignore_ascii_case("ALTER") {
        Some(CommandTag::Alter(object))
    } else {
        None
    }
}

fn transaction_control_tag(query: &str) -> Option<CommandTag> {
    match first_keyword(query).to_ascii_uppercase().as_str() {
        "BEGIN" => Some(CommandTag::Begin),
        "COMMIT" | "END" => Some(CommandTag::Commit),
        "ROLLBACK" if is_rollback_to(query) => Some(CommandTag::Other(format!(
            "ROLLBACK TO {}",
            rollback_to_target(query).unwrap_or("")
        ))),
        "ROLLBACK" => Some(CommandTag::Rollback),
        "SAVEPOINT" => Some(CommandTag::Other(format!(
            "SAVEPOINT {}",
            savepoint_target(query).unwrap_or("")
        ))),
        "RELEASE" => Some(CommandTag::Other(format!(
            "RELEASE {}",
            savepoint_target(query).unwrap_or("")
        ))),
        _ => None,
    }
}

fn dml_tag(query: &str, affected_rows: usize) -> Option<CommandTag> {
    let affected_rows = affected_rows as u64;
    match dml_keyword(query) {
        Some("INSERT") => Some(CommandTag::Insert(affected_rows)),
        Some("UPDATE") => Some(CommandTag::Update(affected_rows)),
        Some("DELETE") => Some(CommandTag::Delete(affected_rows)),
        _ => None,
    }
}

fn sqlite_side_effect_tag(query: &str) -> Option<CommandTag> {
    let keyword = first_keyword(query).to_ascii_uppercase();
    match keyword.as_str() {
        "ANALYZE" | "ATTACH" | "DETACH" | "REINDEX" | "VACUUM" => Some(CommandTag::Other(keyword)),
        _ => None,
    }
}

pub(in crate::adapters::sqlite::sqlite3) fn sqlite_statement_tags(
    statements: &[&str],
    changes: &HashMap<usize, usize>,
) -> Vec<CommandTag> {
    statements
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            dml_tag(statement, *changes.get(&index).unwrap_or(&0))
                .or_else(|| ddl_tag(statement))
                .or_else(|| transaction_control_tag(statement))
                .or_else(|| sqlite_side_effect_tag(statement))
        })
        .collect()
}

pub(in crate::adapters::sqlite::sqlite3) fn discard_rolled_back(
    tags: &[CommandTag],
) -> Vec<CommandTag> {
    let mut effective = Vec::new();
    let mut frames: Vec<(Option<String>, Vec<CommandTag>)> = Vec::new();

    for tag in tags {
        match tag {
            CommandTag::Begin => frames.push((None, Vec::new())),
            CommandTag::Other(raw) if raw == "SAVEPOINT" || raw.starts_with("SAVEPOINT ") => {
                frames.push((tag_name(raw, "SAVEPOINT"), Vec::new()));
            }
            CommandTag::Other(raw) if raw == "RELEASE" || raw.starts_with("RELEASE ") => {
                if let Some(index) = savepoint_frame_index(&frames, tag_name(raw, "RELEASE")) {
                    let mut merged = Vec::new();
                    for (_, frame) in frames.drain(index..) {
                        merged.extend(frame);
                    }
                    if let Some((_, parent)) = frames.last_mut() {
                        parent.extend(merged);
                    } else {
                        effective.extend(merged);
                    }
                }
            }
            CommandTag::Other(raw) if raw == "ROLLBACK TO" || raw.starts_with("ROLLBACK TO ") => {
                if let Some(index) = savepoint_frame_index(&frames, tag_name(raw, "ROLLBACK TO")) {
                    frames.truncate(index + 1);
                    if let Some((_, frame)) = frames.last_mut() {
                        frame.clear();
                    }
                }
            }
            CommandTag::Rollback => {
                frames.clear();
            }
            CommandTag::Commit => {
                for (_, frame) in frames.drain(..) {
                    effective.extend(frame);
                }
            }
            _ => {
                if let Some((_, frame)) = frames.last_mut() {
                    frame.push(tag.clone());
                } else {
                    effective.push(tag.clone());
                }
            }
        }
    }

    effective
}

fn tag_name(raw: &str, prefix: &str) -> Option<String> {
    raw.strip_prefix(prefix)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_uppercase)
}

fn savepoint_frame_index(
    frames: &[(Option<String>, Vec<CommandTag>)],
    name: Option<String>,
) -> Option<usize> {
    frames
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, (frame_name, _))| {
            if frame_name.is_none() && index == 0 {
                return None;
            }
            if name
                .as_ref()
                .is_none_or(|name| frame_name.as_ref() == Some(name))
            {
                Some(index)
            } else {
                None
            }
        })
}

pub(in crate::adapters::sqlite::sqlite3) fn aggregate_sqlite_command_tag(
    tags: &[CommandTag],
) -> Option<CommandTag> {
    let effective = discard_rolled_back(tags);
    if let Some(tag) = effective.iter().find(|tag| tag.is_schema_modifying()) {
        return Some(tag.clone());
    }
    if let Some(tag) = effective.iter().rev().find(|tag| tag.needs_refresh()) {
        return Some(tag.clone());
    }
    if tags.iter().any(CommandTag::needs_refresh) {
        return Some(CommandTag::Rollback);
    }
    tags.last().cloned()
}

pub(in crate::adapters::sqlite::sqlite3) fn command_tag_result(
    query: &str,
    tag: CommandTag,
    elapsed: u64,
    source: QuerySource,
) -> QueryResult {
    QueryResult::success(query.to_string(), Vec::new(), Vec::new(), elapsed, source)
        .with_row_count(tag.affected_rows().unwrap_or(0) as usize)
        .with_command_tag(tag)
}

pub(in crate::adapters::sqlite::sqlite3) fn statement_counts_as_select_tag(
    statement: &str,
) -> bool {
    let keyword = first_keyword(statement);
    keyword.eq_ignore_ascii_case("SELECT") || keyword.eq_ignore_ascii_case("WITH")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::domain::CommandTag;

    use super::*;

    mod discard_rolled_back_policy {
        use super::*;

        fn sp(name: &str) -> CommandTag {
            CommandTag::Other(format!("SAVEPOINT {name}"))
        }

        fn rollback_to(name: &str) -> CommandTag {
            CommandTag::Other(format!("ROLLBACK TO {name}"))
        }

        fn release(name: &str) -> CommandTag {
            CommandTag::Other(format!("RELEASE {name}"))
        }

        #[test]
        fn top_level_savepoint_rollback_to_discards_inner_dml() {
            let tags = vec![
                sp("sp"),
                CommandTag::Insert(1),
                rollback_to("sp"),
                CommandTag::Insert(2),
                release("sp"),
            ];

            assert_eq!(discard_rolled_back(&tags), vec![CommandTag::Insert(2)]);
        }

        #[test]
        fn top_level_savepoint_full_rollback_discards_all_dml() {
            let tags = vec![sp("sp"), CommandTag::Insert(1), CommandTag::Rollback];

            assert!(discard_rolled_back(&tags).is_empty());
        }

        #[test]
        fn unclosed_begin_discards_frame_from_effective() {
            let tags = vec![CommandTag::Begin, CommandTag::Update(1)];

            assert!(discard_rolled_back(&tags).is_empty());
        }

        #[test]
        fn unclosed_top_level_savepoint_discards_frame_from_effective() {
            let tags = vec![sp("sp"), CommandTag::Insert(1)];

            assert!(discard_rolled_back(&tags).is_empty());
        }

        #[test]
        fn unclosed_top_level_savepoint_aggregates_to_rollback() {
            let tags = vec![sp("sp"), CommandTag::Insert(1)];

            assert_eq!(
                aggregate_sqlite_command_tag(&tags),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn rollback_to_quoted_savepoint_is_not_full_rollback() {
            let tags = sqlite_statement_tags(
                &[
                    "SAVEPOINT sp",
                    "INSERT INTO users(id) VALUES (1)",
                    "ROLLBACK TO \"sp\"",
                ],
                &HashMap::from([(1, 1)]),
            );

            assert_eq!(
                tags,
                vec![
                    CommandTag::Other("SAVEPOINT sp".to_string()),
                    CommandTag::Insert(1),
                    CommandTag::Other("ROLLBACK TO \"sp\"".to_string()),
                ]
            );
            assert!(discard_rolled_back(&tags).is_empty());
        }

        #[test]
        fn quoted_savepoint_release_merges_named_frame() {
            let tags = sqlite_statement_tags(
                &[
                    "SAVEPOINT \"outer\"",
                    "SAVEPOINT \"inner\"",
                    "INSERT INTO users(id) VALUES (1)",
                    "RELEASE \"outer\"",
                ],
                &HashMap::from([(2, 1)]),
            );

            assert_eq!(discard_rolled_back(&tags), vec![CommandTag::Insert(1)]);
        }

        #[test]
        fn release_savepoint_uses_following_target_name() {
            let tags = sqlite_statement_tags(
                &[
                    "SAVEPOINT outer",
                    "SAVEPOINT inner",
                    "INSERT INTO users(id) VALUES (1)",
                    "RELEASE SAVEPOINT outer",
                ],
                &HashMap::from([(2, 1)]),
            );

            assert_eq!(discard_rolled_back(&tags), vec![CommandTag::Insert(1)]);
        }

        #[test]
        fn begin_savepoint_release_still_merges_nested_frame() {
            let tags = vec![
                CommandTag::Begin,
                sp("inner"),
                CommandTag::Insert(1),
                release("inner"),
                CommandTag::Commit,
            ];

            assert_eq!(discard_rolled_back(&tags), vec![CommandTag::Insert(1)]);
        }

        #[test]
        fn rollback_transaction_to_savepoint_discards_inner_dml() {
            let tags = sqlite_statement_tags(
                &[
                    "SAVEPOINT sp",
                    "INSERT INTO users(id) VALUES (1)",
                    "INSERT INTO users(id) VALUES (2)",
                    "ROLLBACK TRANSACTION TO SAVEPOINT sp",
                    "INSERT INTO users(id) VALUES (3)",
                    "RELEASE sp",
                ],
                &HashMap::from([(1, 1), (2, 1), (4, 1)]),
            );

            assert_eq!(discard_rolled_back(&tags), vec![CommandTag::Insert(1)]);
        }
    }

    #[test]
    fn sqlite_side_effect_statements_emit_refresh_tags() {
        let changes = HashMap::new();

        let tags = sqlite_statement_tags(
            &[
                "ANALYZE users",
                "ATTACH DATABASE 'other.db' AS other",
                "DETACH DATABASE other",
                "REINDEX users_name_idx",
                "VACUUM",
            ],
            &changes,
        );

        assert_eq!(
            tags,
            vec![
                CommandTag::Other("ANALYZE".to_string()),
                CommandTag::Other("ATTACH".to_string()),
                CommandTag::Other("DETACH".to_string()),
                CommandTag::Other("REINDEX".to_string()),
                CommandTag::Other("VACUUM".to_string()),
            ]
        );
        assert!(tags.iter().all(CommandTag::needs_refresh));
    }
}
