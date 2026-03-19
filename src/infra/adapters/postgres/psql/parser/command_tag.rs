use crate::domain::CommandTag;

use super::super::super::PostgresAdapter;
use super::lexer::{has_select_into, split_sql_statements};

impl PostgresAdapter {
    pub(in crate::infra::adapters::postgres) fn parse_command_tag(tag: &str) -> Option<CommandTag> {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return None;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        match parts.first().copied()? {
            "SELECT" => {
                let n = parts.get(1)?.parse::<u64>().ok()?;
                Some(CommandTag::Select(n))
            }
            "INSERT" => {
                // INSERT oid count — count is at index 2
                let n = parts.get(2)?.parse::<u64>().ok()?;
                Some(CommandTag::Insert(n))
            }
            "UPDATE" => {
                let n = parts.get(1)?.parse::<u64>().ok()?;
                Some(CommandTag::Update(n))
            }
            "DELETE" => {
                let n = parts.get(1)?.parse::<u64>().ok()?;
                Some(CommandTag::Delete(n))
            }
            "CREATE" => {
                let obj = parts.get(1)?;
                Some(CommandTag::Create(obj.to_string()))
            }
            "DROP" => {
                let obj = parts.get(1)?;
                Some(CommandTag::Drop(obj.to_string()))
            }
            "ALTER" => {
                let obj = parts.get(1)?;
                Some(CommandTag::Alter(obj.to_string()))
            }
            "TRUNCATE" => Some(CommandTag::Truncate),
            "BEGIN" => Some(CommandTag::Begin),
            "COMMIT" => Some(CommandTag::Commit),
            "ROLLBACK" => Some(CommandTag::Rollback),
            _ => Some(CommandTag::Other(trimmed.to_string())),
        }
    }

    pub(in crate::infra::adapters::postgres) fn extract_command_tag(
        stdout: &str,
    ) -> Option<CommandTag> {
        stdout
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .and_then(Self::parse_command_tag)
    }

    fn is_known_tcl_tag(s: &str) -> bool {
        matches!(
            s.split_whitespace().next().unwrap_or(""),
            "BEGIN" | "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE"
        ) || s == "START TRANSACTION"
    }

    fn parse_all_tags(stdout: &str) -> Option<Vec<CommandTag>> {
        let mut tags = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let tag = Self::parse_command_tag(trimmed)?;
            if let CommandTag::Other(ref raw) = tag
                && !Self::is_known_tcl_tag(raw)
            {
                return None;
            }
            tags.push(tag);
        }
        if tags.is_empty() {
            return None;
        }
        Some(tags)
    }

    fn discard_rolled_back(tags: &[CommandTag]) -> Vec<CommandTag> {
        let mut effective: Vec<CommandTag> = Vec::new();
        let mut frames: Vec<Vec<CommandTag>> = Vec::new();

        for tag in tags {
            match tag {
                CommandTag::Begin => {
                    frames.push(Vec::new());
                }
                CommandTag::Other(raw) if raw == "START TRANSACTION" => {
                    frames.push(Vec::new());
                }
                CommandTag::Other(raw) if raw == "SAVEPOINT" || raw.starts_with("SAVEPOINT ") => {
                    frames.push(Vec::new());
                }
                CommandTag::Other(raw) if raw == "RELEASE" || raw.starts_with("RELEASE ") => {
                    // Only merge a savepoint frame (depth > 1).
                    // At depth <= 1 the savepoint was already popped by ROLLBACK.
                    if frames.len() > 1
                        && let Some(inner) = frames.pop()
                    {
                        if let Some(parent) = frames.last_mut() {
                            parent.extend(inner);
                        } else {
                            effective.extend(inner);
                        }
                    }
                }
                CommandTag::Rollback => {
                    if frames.len() > 1 {
                        frames.pop();
                    } else {
                        frames.clear();
                    }
                }
                CommandTag::Commit => {
                    for frame in frames.drain(..) {
                        effective.extend(frame);
                    }
                }
                _ => {
                    if let Some(frame) = frames.last_mut() {
                        frame.push(tag.clone());
                    } else {
                        effective.push(tag.clone());
                    }
                }
            }
        }

        // Unclosed transaction: treat remaining frames as effective
        for frame in frames.drain(..) {
            effective.extend(frame);
        }

        effective
    }

    // -- CTAS / SELECT INTO correction (newspaper style: high→low) --

    pub(in crate::infra::adapters::postgres) fn parse_aggregate_command_tag(
        stdout: &str,
        sql: &str,
    ) -> Option<CommandTag> {
        ResolvedTags::resolve(stdout, sql)?.aggregate()
    }

    fn correct_ctas_tags(sql: &str, tags: Vec<CommandTag>) -> Vec<CommandTag> {
        let stmts = split_sql_statements(sql);
        if stmts.len() != tags.len() {
            return tags;
        }
        tags.into_iter()
            .zip(stmts.iter())
            .map(|(tag, stmt)| {
                if !matches!(tag, CommandTag::Select(_)) {
                    return tag;
                }
                Self::detect_create_as_kind(stmt)
                    .or_else(|| Self::detect_select_into_kind(stmt))
                    .unwrap_or(tag)
            })
            .collect()
    }

    fn detect_create_as_kind(stmt: &str) -> Option<CommandTag> {
        let lower = stmt.trim().to_ascii_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        if words.first() != Some(&"create") {
            return None;
        }
        let mut idx = 1;
        while idx < words.len() && matches!(words[idx], "temp" | "temporary" | "unlogged") {
            idx += 1;
        }
        if idx < words.len() && words[idx] == "table" {
            return Some(CommandTag::Create("TABLE".to_string()));
        }
        if idx < words.len() && words[idx] == "materialized" && words.get(idx + 1) == Some(&"view")
        {
            return Some(CommandTag::Create("MATERIALIZED VIEW".to_string()));
        }
        None
    }

    fn detect_select_into_kind(stmt: &str) -> Option<CommandTag> {
        let lower = stmt.trim().to_ascii_lowercase();
        let first = lower.split_whitespace().next()?;
        if !matches!(first, "select" | "with") {
            return None;
        }
        has_select_into(&lower).then(|| CommandTag::Create("TABLE".to_string()))
    }
}

#[cfg_attr(test, derive(Debug))]
pub(in crate::infra::adapters::postgres) struct ResolvedTags {
    all: Vec<CommandTag>,
    effective: Vec<CommandTag>,
}

impl ResolvedTags {
    pub(in crate::infra::adapters::postgres) fn resolve(stdout: &str, sql: &str) -> Option<Self> {
        let parsed = PostgresAdapter::parse_all_tags(stdout)?;
        let corrected = PostgresAdapter::correct_ctas_tags(sql, parsed);
        let effective = PostgresAdapter::discard_rolled_back(&corrected);
        Some(Self {
            all: corrected,
            effective,
        })
    }

    pub(in crate::infra::adapters::postgres) fn aggregate(&self) -> Option<CommandTag> {
        if let Some(tag) = self.effective.iter().find(|t| t.is_schema_modifying()) {
            return Some(tag.clone());
        }

        if let Some(tag) = self.effective.iter().rev().find(|t| t.needs_refresh()) {
            return Some(tag.clone());
        }

        if self.all.iter().any(|t| t.needs_refresh()) {
            return Some(CommandTag::Rollback);
        }

        self.all.last().cloned()
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::CommandTag;
    use crate::infra::adapters::postgres::PostgresAdapter;

    mod detect_create_as_kind {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case::basic("CREATE TABLE t AS SELECT 1", "TABLE")]
        #[case::temp("CREATE TEMP TABLE t AS SELECT 1", "TABLE")]
        #[case::temporary("CREATE TEMPORARY TABLE t AS SELECT 1", "TABLE")]
        #[case::unlogged("CREATE UNLOGGED TABLE t AS SELECT 1", "TABLE")]
        #[case::lowercase("create table t as select 1", "TABLE")]
        #[case::materialized_view("CREATE MATERIALIZED VIEW v AS SELECT 1", "MATERIALIZED VIEW")]
        fn create_as_returns_correct_tag(#[case] stmt: &str, #[case] object: &str) {
            assert_eq!(
                PostgresAdapter::detect_create_as_kind(stmt),
                Some(CommandTag::Create(object.to_string()))
            );
        }

        #[rstest]
        #[case::create_view("CREATE VIEW v AS SELECT 1")]
        #[case::plain_select("SELECT * FROM users")]
        fn non_ctas_returns_none(#[case] stmt: &str) {
            assert_eq!(PostgresAdapter::detect_create_as_kind(stmt), None);
        }
    }

    mod detect_select_into_kind {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case::basic("SELECT * INTO t FROM users")]
        #[case::with_columns("SELECT id, name INTO t FROM users")]
        #[case::with_cte("WITH cte AS (SELECT 1) SELECT * INTO t FROM cte")]
        fn select_into_returns_create_table(#[case] stmt: &str) {
            assert_eq!(
                PostgresAdapter::detect_select_into_kind(stmt),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[rstest]
        #[case::plain_select("SELECT * FROM users")]
        #[case::insert_into("INSERT INTO users VALUES (1)")]
        #[case::create_table("CREATE TABLE t AS SELECT 1")]
        fn non_select_into_returns_none(#[case] stmt: &str) {
            assert_eq!(PostgresAdapter::detect_select_into_kind(stmt), None);
        }

        #[rstest]
        #[case::double_quotes(r#"SELECT "into" FROM t"#)]
        #[case::dollar_quote("SELECT $$into$$ FROM t")]
        #[case::tagged_dollar_quote("SELECT $x$into$x$ FROM t")]
        #[case::line_comment("SELECT 1 -- into\nFROM t")]
        #[case::block_comment("SELECT /* into */ 1 FROM t")]
        #[case::single_quote("SELECT 'into' FROM t")]
        fn quoted_into_returns_none(#[case] stmt: &str) {
            assert_eq!(PostgresAdapter::detect_select_into_kind(stmt), None);
        }
    }

    mod command_tag_parsing {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("SELECT 5", CommandTag::Select(5))]
        #[case("SELECT 0", CommandTag::Select(0))]
        #[case("INSERT 0 3", CommandTag::Insert(3))]
        #[case("INSERT 0 0", CommandTag::Insert(0))]
        #[case("UPDATE 5", CommandTag::Update(5))]
        #[case("UPDATE 0", CommandTag::Update(0))]
        #[case("DELETE 10", CommandTag::Delete(10))]
        #[case("DELETE 0", CommandTag::Delete(0))]
        #[case("CREATE TABLE", CommandTag::Create("TABLE".to_string()))]
        #[case("CREATE INDEX", CommandTag::Create("INDEX".to_string()))]
        #[case("DROP TABLE", CommandTag::Drop("TABLE".to_string()))]
        #[case("DROP INDEX", CommandTag::Drop("INDEX".to_string()))]
        #[case("ALTER TABLE", CommandTag::Alter("TABLE".to_string()))]
        #[case("TRUNCATE TABLE", CommandTag::Truncate)]
        #[case("BEGIN", CommandTag::Begin)]
        #[case("COMMIT", CommandTag::Commit)]
        #[case("ROLLBACK", CommandTag::Rollback)]
        fn parse_known_tags(#[case] input: &str, #[case] expected: CommandTag) {
            assert_eq!(PostgresAdapter::parse_command_tag(input), Some(expected));
        }

        #[rstest]
        #[case("")]
        #[case("   ")]
        fn empty_or_whitespace_returns_none(#[case] input: &str) {
            assert_eq!(PostgresAdapter::parse_command_tag(input), None);
        }

        #[rstest]
        #[case("SELECT abc")]
        #[case("INSERT 0")]
        #[case("INSERT 0 abc")]
        #[case("UPDATE")]
        #[case("DELETE")]
        fn malformed_count_returns_none(#[case] input: &str) {
            assert_eq!(PostgresAdapter::parse_command_tag(input), None);
        }

        #[test]
        fn unknown_command_returns_other() {
            assert_eq!(
                PostgresAdapter::parse_command_tag("VACUUM"),
                Some(CommandTag::Other("VACUUM".to_string()))
            );
        }

        #[test]
        fn extract_from_multiline_with_notice() {
            let stdout = "NOTICE:  table \"foo\" does not exist, skipping\nDROP TABLE\n";
            assert_eq!(
                PostgresAdapter::extract_command_tag(stdout),
                Some(CommandTag::Drop("TABLE".to_string()))
            );
        }

        #[test]
        fn extract_skips_trailing_empty_lines() {
            let stdout = "INSERT 0 7\n\n  \n";
            assert_eq!(
                PostgresAdapter::extract_command_tag(stdout),
                Some(CommandTag::Insert(7))
            );
        }

        #[test]
        fn extract_from_empty_returns_none() {
            assert_eq!(PostgresAdapter::extract_command_tag(""), None);
            assert_eq!(PostgresAdapter::extract_command_tag("  \n  \n"), None);
        }

        #[test]
        fn parse_affected_rows_regression() {
            assert_eq!(PostgresAdapter::parse_affected_rows("UPDATE 3\n"), Some(3));
            assert_eq!(PostgresAdapter::parse_affected_rows("DELETE 5\n"), Some(5));
            assert_eq!(
                PostgresAdapter::parse_affected_rows("INSERT 0 10\n"),
                Some(10)
            );
            assert_eq!(PostgresAdapter::parse_affected_rows("SELECT 1\n"), Some(1));
        }
    }

    mod is_known_tcl_tag {
        use super::*;

        #[test]
        fn recognizes_begin_commit_rollback() {
            assert!(PostgresAdapter::is_known_tcl_tag("BEGIN"));
            assert!(PostgresAdapter::is_known_tcl_tag("COMMIT"));
            assert!(PostgresAdapter::is_known_tcl_tag("ROLLBACK"));
        }

        #[test]
        fn recognizes_savepoint_and_release() {
            assert!(PostgresAdapter::is_known_tcl_tag("SAVEPOINT"));
            assert!(PostgresAdapter::is_known_tcl_tag("RELEASE"));
        }

        #[test]
        fn recognizes_start_transaction() {
            assert!(PostgresAdapter::is_known_tcl_tag("START TRANSACTION"));
        }

        #[test]
        fn rejects_non_tcl() {
            assert!(!PostgresAdapter::is_known_tcl_tag("UPDATE 1"));
            assert!(!PostgresAdapter::is_known_tcl_tag("id,name"));
            assert!(!PostgresAdapter::is_known_tcl_tag("VACUUM"));
        }
    }

    mod parse_all_tags {
        use super::*;

        #[test]
        fn single_dml() {
            assert_eq!(
                PostgresAdapter::parse_all_tags("DELETE 3"),
                Some(vec![CommandTag::Delete(3)])
            );
        }

        #[test]
        fn single_ddl() {
            assert_eq!(
                PostgresAdapter::parse_all_tags("CREATE TABLE"),
                Some(vec![CommandTag::Create("TABLE".to_string())])
            );
        }

        #[test]
        fn multi_line_tags() {
            assert_eq!(
                PostgresAdapter::parse_all_tags("BEGIN\nUPDATE 1\nCOMMIT"),
                Some(vec![
                    CommandTag::Begin,
                    CommandTag::Update(1),
                    CommandTag::Commit,
                ])
            );
        }

        #[test]
        fn csv_returns_none() {
            assert_eq!(PostgresAdapter::parse_all_tags("id,name\n1,Alice"), None);
        }

        #[test]
        fn empty_string_returns_none() {
            assert_eq!(PostgresAdapter::parse_all_tags(""), None);
        }

        #[test]
        fn skips_empty_lines() {
            assert_eq!(
                PostgresAdapter::parse_all_tags("BEGIN\n\nUPDATE 1\n"),
                Some(vec![CommandTag::Begin, CommandTag::Update(1)])
            );
        }

        #[test]
        fn savepoint_line_is_accepted() {
            let result = PostgresAdapter::parse_all_tags("SAVEPOINT");
            assert!(result.is_some());
            assert_eq!(
                result.unwrap(),
                vec![CommandTag::Other("SAVEPOINT".to_string())]
            );
        }
    }

    mod discard_rolled_back {
        use super::*;

        fn sp() -> CommandTag {
            CommandTag::Other("SAVEPOINT".to_string())
        }
        fn release() -> CommandTag {
            CommandTag::Other("RELEASE".to_string())
        }

        #[test]
        fn committed_txn() {
            let tags = vec![CommandTag::Begin, CommandTag::Update(1), CommandTag::Commit];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1)]
            );
        }

        #[test]
        fn full_rollback() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                CommandTag::Rollback,
            ];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn no_txn() {
            let tags = vec![CommandTag::Update(1)];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1)]
            );
        }

        #[test]
        fn savepoint_release() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                sp(),
                CommandTag::Insert(1),
                release(),
                CommandTag::Commit,
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1), CommandTag::Insert(1)]
            );
        }

        #[test]
        fn partial_rollback() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                sp(),
                CommandTag::Insert(1),
                CommandTag::Rollback,
                CommandTag::Commit,
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1)]
            );
        }

        #[test]
        fn full_rollback_with_savepoint() {
            let tags = vec![
                CommandTag::Begin,
                sp(),
                CommandTag::Create("TABLE".to_string()),
                CommandTag::Rollback,
                CommandTag::Commit,
            ];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn unclosed_txn() {
            let tags = vec![CommandTag::Begin, CommandTag::Update(1)];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1)]
            );
        }

        #[test]
        fn tcl_only() {
            let tags = vec![CommandTag::Begin, CommandTag::Commit];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn rollback_then_dml() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                sp(),
                CommandTag::Insert(1),
                CommandTag::Rollback,
                CommandTag::Delete(3),
                CommandTag::Commit,
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1), CommandTag::Delete(3)]
            );
        }

        #[test]
        fn rollback_then_release_same_sp() {
            let tags = vec![
                CommandTag::Begin,
                sp(),
                CommandTag::Insert(1),
                CommandTag::Rollback,
                release(),
                CommandTag::Commit,
            ];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn release_after_rollback_does_not_leak_outer_frame() {
            // C2 regression: RELEASE after ROLLBACK must not pop the
            // transaction frame and leak its contents into effective.
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                sp(),
                CommandTag::Insert(1),
                CommandTag::Rollback,
                release(),
                CommandTag::Rollback,
            ];
            let effective = PostgresAdapter::discard_rolled_back(&tags);
            assert!(effective.is_empty());
        }

        #[test]
        fn multiple_txns() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                CommandTag::Commit,
                CommandTag::Begin,
                CommandTag::Insert(1),
                CommandTag::Commit,
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Update(1), CommandTag::Insert(1)]
            );
        }

        #[test]
        fn full_rollback_then_bare_dml() {
            let tags = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                CommandTag::Rollback,
                CommandTag::Delete(3),
            ];
            assert_eq!(
                PostgresAdapter::discard_rolled_back(&tags),
                vec![CommandTag::Delete(3)]
            );
        }
    }

    mod resolved_tags_aggregate {
        use super::super::ResolvedTags;
        use super::*;

        fn resolved(all: Vec<CommandTag>, effective: Vec<CommandTag>) -> ResolvedTags {
            ResolvedTags { all, effective }
        }

        #[test]
        fn schema_modifying_takes_priority() {
            let tags = vec![CommandTag::Drop("TABLE".to_string()), CommandTag::Delete(1)];
            assert_eq!(
                resolved(tags.clone(), tags).aggregate(),
                Some(CommandTag::Drop("TABLE".to_string()))
            );
        }

        #[test]
        fn last_needs_refresh() {
            let tags = vec![CommandTag::Insert(1), CommandTag::Update(2)];
            assert_eq!(
                resolved(tags.clone(), tags).aggregate(),
                Some(CommandTag::Update(2))
            );
        }

        #[test]
        fn effective_empty_with_modifying_returns_rollback() {
            let all = vec![
                CommandTag::Begin,
                CommandTag::Update(1),
                CommandTag::Rollback,
            ];
            assert_eq!(
                resolved(all, vec![]).aggregate(),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn tcl_only_returns_last_tag() {
            let all = vec![CommandTag::Begin, CommandTag::Commit];
            assert_eq!(resolved(all, vec![]).aggregate(), Some(CommandTag::Commit));
        }

        #[test]
        fn ddl_and_dml_mixed_schema_wins() {
            let tags = vec![
                CommandTag::Create("TABLE".to_string()),
                CommandTag::Insert(1),
            ];
            assert_eq!(
                resolved(tags.clone(), tags).aggregate(),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }
    }

    mod parse_aggregate_command_tag {
        use super::*;

        #[test]
        fn committed_txn_with_update() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nUPDATE 1\nCOMMIT",
                    "BEGIN; UPDATE t SET x=1; COMMIT"
                ),
                Some(CommandTag::Update(1))
            );
        }

        #[test]
        fn rolled_back_txn() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nUPDATE 1\nROLLBACK",
                    "BEGIN; UPDATE t SET x=1; ROLLBACK"
                ),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn partial_rollback_keeps_outer_dml() {
            let stdout = "BEGIN\nUPDATE 1\nSAVEPOINT\nINSERT 0 1\nROLLBACK\nCOMMIT";
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    stdout,
                    "BEGIN; UPDATE t SET x=1; SAVEPOINT s; INSERT INTO t VALUES(1); ROLLBACK TO SAVEPOINT s; COMMIT"
                ),
                Some(CommandTag::Update(1))
            );
        }

        #[test]
        fn single_dml() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag("DELETE 3", "DELETE FROM t"),
                Some(CommandTag::Delete(3))
            );
        }

        #[test]
        fn csv_returns_none() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag("id,name", "SELECT * FROM t"),
                None
            );
        }

        #[test]
        fn single_line_select_passes_through() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag("SELECT 5", "SELECT 1+4"),
                Some(CommandTag::Select(5))
            );
        }

        #[test]
        fn ctas_committed() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSELECT 1\nCOMMIT",
                    "BEGIN; CREATE TABLE t AS SELECT 1; COMMIT"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn ctas_savepoint_rollback_with_outer_dml() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSAVEPOINT\nSELECT 1\nROLLBACK\nUPDATE 1\nCOMMIT",
                    "BEGIN; SAVEPOINT s; CREATE TABLE t AS SELECT 1; ROLLBACK TO SAVEPOINT s; UPDATE t SET x=1; COMMIT"
                ),
                Some(CommandTag::Update(1))
            );
        }

        #[test]
        fn ctas_savepoint_rollback_no_other_dml() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSAVEPOINT\nSELECT 1\nROLLBACK\nCOMMIT",
                    "BEGIN; SAVEPOINT s; CREATE TABLE t AS SELECT 1; ROLLBACK TO SAVEPOINT s; COMMIT"
                ),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn select_into_savepoint_rollback() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSAVEPOINT\nSELECT 5\nROLLBACK\nCOMMIT",
                    "BEGIN; SAVEPOINT s; SELECT * INTO t FROM u; ROLLBACK TO SAVEPOINT s; COMMIT"
                ),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn single_ctas_no_txn() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1",
                    "CREATE TABLE t AS SELECT 1"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn ctas_outside_savepoint_survives() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSELECT 1\nCOMMIT",
                    "BEGIN; CREATE TABLE t AS SELECT 1; COMMIT"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn ctas_full_rollback() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "BEGIN\nSELECT 1\nROLLBACK",
                    "BEGIN; CREATE TABLE t AS SELECT 1; ROLLBACK"
                ),
                Some(CommandTag::Rollback)
            );
        }

        #[test]
        fn create_temp_table_as() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1",
                    "CREATE TEMP TABLE t AS SELECT 1"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn create_materialized_view() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1",
                    "CREATE MATERIALIZED VIEW v AS SELECT 1"
                ),
                Some(CommandTag::Create("MATERIALIZED VIEW".to_string()))
            );
        }

        #[test]
        fn with_select_into() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1",
                    "WITH cte AS (SELECT 1) SELECT * INTO t FROM cte"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn ctas_then_delete_no_txn() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 1\nDELETE 1",
                    "CREATE TABLE t AS SELECT 1; DELETE FROM users WHERE id = 1"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn select_into_then_delete_no_txn() {
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag(
                    "SELECT 5\nDELETE 1",
                    "SELECT * INTO t FROM users; DELETE FROM users WHERE id = 1"
                ),
                Some(CommandTag::Create("TABLE".to_string()))
            );
        }

        #[test]
        fn count_mismatch_fallback() {
            // 1 statement but 2 tags → skip correction, use last tag
            assert_eq!(
                PostgresAdapter::parse_aggregate_command_tag("SELECT 1\nSELECT 2", "SELECT 1"),
                Some(CommandTag::Select(2))
            );
        }
    }
}
