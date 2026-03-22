use super::statement_classifier::{
    StatementKind, advance_single_quote, classify, extract_table_name, skip_block_comment,
    skip_dollar_quoted_string, skip_double_quoted_identifier, skip_line_comment,
};
use super::write_guardrails::RiskLevel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationType {
    Immediate,
    Enter,
    TableNameInput { target: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlRiskDecision {
    pub risk_level: RiskLevel,
    pub confirmation: ConfirmationType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiStatementDecision {
    Allow {
        statements: Vec<String>,
        risk: SqlRiskDecision,
    },
    Block {
        reason: String,
    },
}

pub fn split_statements(sql: &str) -> Vec<String> {
    // Use sql's own char_indices so byte offsets remain valid for slicing sql.
    // to_lowercase() can change byte lengths (e.g. İ → i̇), which would corrupt offsets.
    let chars: Vec<(usize, char)> = sql.char_indices().collect();
    let mut statements = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut in_string = false;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];

        if let Some(next_i) = skip_line_comment(&chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_block_comment(&chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = advance_single_quote(&chars, i, ch, &mut in_string) {
            i = next_i;
            continue;
        }
        if in_string {
            i += 1;
            continue;
        }
        if let Some(next_i) = skip_double_quoted_identifier(&chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_dollar_quoted_string(sql, &chars, i, byte_pos, ch) {
            i = next_i;
            continue;
        }

        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
        }

        if depth == 0 && ch == ';' {
            let fragment = sql[start..byte_pos].trim();
            if !fragment.is_empty() {
                statements.push(fragment.to_string());
            }
            start = byte_pos + 1;
        }

        i += 1;
    }

    if start < sql.len() {
        let fragment = sql[start..].trim();
        if !fragment.is_empty() {
            statements.push(fragment.to_string());
        }
    }

    statements.retain(|s| !is_comment_only(s));

    statements
}

fn is_comment_only(sql: &str) -> bool {
    let chars: Vec<(usize, char)> = sql.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let (_byte_pos, ch) = chars[i];

        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if let Some(next_i) = skip_line_comment(&chars, i, ch) {
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_block_comment(&chars, i, ch) {
            i = next_i;
            continue;
        }
        return false;
    }
    true
}

pub fn evaluate_sql_risk(kind: &StatementKind, sql: &str) -> Option<SqlRiskDecision> {
    match kind {
        StatementKind::Select | StatementKind::Transaction => Some(SqlRiskDecision {
            risk_level: RiskLevel::Low,
            confirmation: ConfirmationType::Immediate,
        }),
        StatementKind::Insert | StatementKind::Create => Some(SqlRiskDecision {
            risk_level: RiskLevel::Low,
            confirmation: ConfirmationType::Enter,
        }),
        StatementKind::Update { has_where: true }
        | StatementKind::Delete { has_where: true }
        | StatementKind::Alter => Some(SqlRiskDecision {
            risk_level: RiskLevel::Medium,
            confirmation: ConfirmationType::Enter,
        }),
        StatementKind::Unsupported => Some(SqlRiskDecision {
            risk_level: RiskLevel::High,
            confirmation: ConfirmationType::Enter,
        }),
        StatementKind::Update { has_where: false }
        | StatementKind::Delete { has_where: false }
        | StatementKind::Drop
        | StatementKind::Truncate => {
            let table = extract_table_name(sql, kind)?;
            Some(SqlRiskDecision {
                risk_level: RiskLevel::High,
                confirmation: ConfirmationType::TableNameInput { target: table },
            })
        }
        // None signals unconditional block; no table name can be extracted for Other.
        StatementKind::Other => None,
    }
}

pub fn evaluate_multi_statement(sql: &str) -> MultiStatementDecision {
    let statements = split_statements(sql);

    if statements.is_empty() {
        return MultiStatementDecision::Block {
            reason: "Empty input".to_string(),
        };
    }

    let mut decisions: Vec<(String, SqlRiskDecision)> = Vec::new();

    for stmt in &statements {
        let kind = classify(stmt);
        match evaluate_sql_risk(&kind, stmt) {
            Some(decision) => decisions.push((stmt.clone(), decision)),
            None => {
                return MultiStatementDecision::Block {
                    reason: "Cannot determine table name for high-risk statement".to_string(),
                };
            }
        }
    }

    let high_count = decisions
        .iter()
        .filter(|(_, d)| d.risk_level == RiskLevel::High)
        .count();
    if high_count >= 2 {
        return MultiStatementDecision::Block {
            reason: "Multiple high-risk statements: execute one at a time".to_string(),
        };
    }

    // Aggregate risk_level and confirmation independently so that a mix like
    // INSERT+SELECT (both Low) yields Enter, not Immediate.
    let max_risk = decisions.iter().map(|(_, d)| d.risk_level).max().unwrap();
    let confirmation = if max_risk == RiskLevel::High {
        // Exactly one HIGH statement (2+ were blocked above); carry its target.
        decisions
            .iter()
            .find(|(_, d)| d.risk_level == RiskLevel::High)
            .map(|(_, d)| d.confirmation.clone())
            .unwrap()
    } else if decisions
        .iter()
        .any(|(_, d)| matches!(d.confirmation, ConfirmationType::Enter))
    {
        ConfirmationType::Enter
    } else if statements.len() >= 2 {
        // Multi-statement: require explicit confirmation even for read-only batches,
        // because the executor produces a single merged result set.
        ConfirmationType::Enter
    } else {
        ConfirmationType::Immediate
    };

    MultiStatementDecision::Allow {
        statements,
        risk: SqlRiskDecision {
            risk_level: max_risk,
            confirmation,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod split_statements_tests {
        use super::*;

        #[rstest]
        #[case::single("SELECT 1", vec!["SELECT 1"])]
        #[case::two("SELECT 1; SELECT 2", vec!["SELECT 1", "SELECT 2"])]
        #[case::trailing_semicolon("SELECT 1;", vec!["SELECT 1"])]
        #[case::empty("", Vec::<&str>::new())]
        #[case::whitespace_only("   ", Vec::<&str>::new())]
        fn basic_split(#[case] sql: &str, #[case] expected: Vec<&str>) {
            assert_eq!(split_statements(sql), expected);
        }

        #[rstest]
        #[case::single_quote("SELECT 'a;b'", vec!["SELECT 'a;b'"])]
        #[case::double_quote("SELECT \"a;b\"", vec!["SELECT \"a;b\""])]
        #[case::dollar_quote("SELECT $$a;b$$", vec!["SELECT $$a;b$$"])]
        #[case::tagged_dollar_quote("SELECT $tag$a;b$tag$", vec!["SELECT $tag$a;b$tag$"])]
        fn semicolon_in_strings(#[case] sql: &str, #[case] expected: Vec<&str>) {
            assert_eq!(split_statements(sql), expected);
        }

        #[rstest]
        #[case::line_comment("SELECT 1 -- ;comment\n; SELECT 2", vec!["SELECT 1 -- ;comment", "SELECT 2"])]
        #[case::block_comment("SELECT /* ; */ 1; SELECT 2", vec!["SELECT /* ; */ 1", "SELECT 2"])]
        fn semicolon_in_comments(#[case] sql: &str, #[case] expected: Vec<&str>) {
            assert_eq!(split_statements(sql), expected);
        }

        #[test]
        fn do_block_split() {
            let sql = "DO $$ BEGIN RAISE NOTICE 'hi'; END $$; SELECT 1";
            let result = split_statements(sql);
            assert_eq!(result.len(), 2);
            assert_eq!(result[0], "DO $$ BEGIN RAISE NOTICE 'hi'; END $$");
            assert_eq!(result[1], "SELECT 1");
        }

        #[test]
        fn escaped_quote_no_split() {
            let sql = "SELECT 'it''s;here'";
            let result = split_statements(sql);
            assert_eq!(result, vec!["SELECT 'it''s;here'"]);
        }

        #[test]
        fn trailing_comment_only() {
            let sql = "SELECT 1; -- comment";
            let result = split_statements(sql);
            assert_eq!(result, vec!["SELECT 1"]);
        }

        #[test]
        fn comment_only_input() {
            let sql = "-- just a comment";
            let result = split_statements(sql);
            assert!(result.is_empty());
        }

        #[test]
        fn unclosed_quote() {
            let sql = "SELECT 'unclosed";
            let result = split_statements(sql);
            assert_eq!(result, vec!["SELECT 'unclosed"]);
        }

        #[test]
        fn non_ascii_before_semicolon() {
            // Case-folding of İ (U+0130) changes byte length in lowercase.
            // Byte offsets must come from the original sql, not the lowercased copy.
            let sql = "SELECT 'İ'; SELECT 2";
            let result = split_statements(sql);
            assert_eq!(result, vec!["SELECT 'İ'", "SELECT 2"]);
        }
    }

    mod evaluate_sql_risk_tests {
        use super::*;

        #[rstest]
        #[case::select(StatementKind::Select, "SELECT 1", RiskLevel::Low, true)]
        #[case::transaction(StatementKind::Transaction, "BEGIN", RiskLevel::Low, true)]
        fn immediate(
            #[case] kind: StatementKind,
            #[case] sql: &str,
            #[case] expected_risk: RiskLevel,
            #[case] is_immediate: bool,
        ) {
            let result = evaluate_sql_risk(&kind, sql).unwrap();
            assert_eq!(result.risk_level, expected_risk);
            assert_eq!(
                matches!(result.confirmation, ConfirmationType::Immediate),
                is_immediate
            );
        }

        #[rstest]
        #[case::insert(StatementKind::Insert, "INSERT INTO users VALUES (1)")]
        #[case::create(StatementKind::Create, "CREATE TABLE t (id INT)")]
        fn low_enter(#[case] kind: StatementKind, #[case] sql: &str) {
            let result = evaluate_sql_risk(&kind, sql).unwrap();
            assert_eq!(result.risk_level, RiskLevel::Low);
            assert!(matches!(result.confirmation, ConfirmationType::Enter));
        }

        #[rstest]
        #[case::update_where(StatementKind::Update { has_where: true }, "UPDATE users SET x=1 WHERE id=1")]
        #[case::delete_where(StatementKind::Delete { has_where: true }, "DELETE FROM users WHERE id=1")]
        #[case::alter(StatementKind::Alter, "ALTER TABLE users ADD COLUMN x INT")]
        fn medium_enter(#[case] kind: StatementKind, #[case] sql: &str) {
            let result = evaluate_sql_risk(&kind, sql).unwrap();
            assert_eq!(result.risk_level, RiskLevel::Medium);
            assert!(matches!(result.confirmation, ConfirmationType::Enter));
        }

        #[test]
        fn unsupported_is_high_enter() {
            let result = evaluate_sql_risk(
                &StatementKind::Unsupported,
                "GRANT SELECT ON users TO role1",
            )
            .unwrap();
            assert_eq!(result.risk_level, RiskLevel::High);
            assert!(matches!(result.confirmation, ConfirmationType::Enter));
        }

        #[rstest]
        #[case::update_no_where(StatementKind::Update { has_where: false }, "UPDATE users SET x=1")]
        #[case::delete_no_where(StatementKind::Delete { has_where: false }, "DELETE FROM users")]
        #[case::drop(StatementKind::Drop, "DROP TABLE users")]
        #[case::truncate(StatementKind::Truncate, "TRUNCATE users")]
        fn high_table_name_input(#[case] kind: StatementKind, #[case] sql: &str) {
            let result = evaluate_sql_risk(&kind, sql).unwrap();
            assert_eq!(result.risk_level, RiskLevel::High);
            assert!(matches!(
                result.confirmation,
                ConfirmationType::TableNameInput { .. }
            ));
        }

        #[test]
        fn other_returns_none() {
            let result = evaluate_sql_risk(&StatementKind::Other, "??? invalid");
            assert!(result.is_none());
        }
    }

    mod evaluate_multi_statement_tests {
        use super::*;

        #[test]
        fn single_select_passthrough() {
            let result = evaluate_multi_statement("SELECT 1");
            match result {
                MultiStatementDecision::Allow { statements, risk } => {
                    assert_eq!(statements, vec!["SELECT 1"]);
                    assert_eq!(risk.confirmation, ConfirmationType::Immediate);
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn single_insert_passthrough() {
            let result = evaluate_multi_statement("INSERT INTO users VALUES (1)");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(risk.confirmation, ConfirmationType::Enter));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn single_drop_passthrough() {
            let result = evaluate_multi_statement("DROP TABLE users");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::High);
                    assert!(matches!(
                        risk.confirmation,
                        ConfirmationType::TableNameInput { .. }
                    ));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn tcl_only_multi_requires_enter() {
            let result = evaluate_multi_statement("BEGIN; COMMIT");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.confirmation, ConfirmationType::Enter);
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn multiple_high_blocked() {
            let result = evaluate_multi_statement("DROP TABLE a; DROP TABLE b");
            match result {
                MultiStatementDecision::Block { reason } => {
                    assert!(reason.contains("Multiple high-risk"));
                }
                _ => panic!("expected Block"),
            }
        }

        #[test]
        fn table_name_extraction_failure_blocked() {
            let result = evaluate_multi_statement("SELECT * INTO backup FROM users");
            match result {
                MultiStatementDecision::Block { reason } => {
                    assert!(reason.contains("table name"));
                }
                _ => panic!("expected Block"),
            }
        }

        #[test]
        fn risk_aggregation_select_insert() {
            let result = evaluate_multi_statement("SELECT 1; INSERT INTO users VALUES (1)");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(risk.confirmation, ConfirmationType::Enter));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn risk_aggregation_select_update_where() {
            let result = evaluate_multi_statement("SELECT 1; UPDATE users SET x = 1 WHERE id = 1");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Medium);
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn empty_input_blocked() {
            let result = evaluate_multi_statement("");
            assert!(matches!(result, MultiStatementDecision::Block { .. }));
        }

        #[test]
        fn do_block_unsupported_high() {
            let result = evaluate_multi_statement("DO $$ BEGIN RAISE NOTICE 'hi'; END $$");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::High);
                    assert!(matches!(risk.confirmation, ConfirmationType::Enter));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn copy_unsupported_high() {
            let result = evaluate_multi_statement("COPY users FROM '/tmp/data.csv'");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::High);
                    assert!(matches!(risk.confirmation, ConfirmationType::Enter));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn insert_then_select_requires_enter_not_immediate() {
            // Both are Low risk, but INSERT needs Enter. The confirmation must be
            // aggregated independently of risk_level to avoid a guard bypass.
            let result = evaluate_multi_statement("INSERT INTO users VALUES (1); SELECT 1");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(risk.confirmation, ConfirmationType::Enter));
                }
                _ => panic!("expected Allow"),
            }
        }
    }
}
