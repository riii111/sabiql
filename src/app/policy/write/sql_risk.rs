use super::write_guardrails::{self, RiskLevel};
use crate::policy::sql::statement_classifier::{
    StatementKind, advance_single_quote, classify, drop_subtype, extract_target_name,
    first_keyword, skip_block_comment, skip_dollar_quoted_string, skip_double_quoted_identifier,
    skip_line_comment,
};

// Why the statement cannot be confirmed via typed target name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcknowledgeReason {
    // The classifier cannot assess the statement (Unsupported / unparseable
    // input), so the worst case cannot be ruled out.
    UnknownRisk,
    // Risk is known to be high, but no target name could be extracted for
    // typed-name confirmation.
    TargetNameUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationType {
    Immediate,
    // Fallback for statements that cannot offer typed-name confirmation.
    Acknowledge {
        reason: AcknowledgeReason,
        label: String,
    },
    TableNameInput {
        target: String,
    },
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

pub fn contains_cli_meta_command(sql: &str) -> bool {
    let chars: Vec<(usize, char)> = sql.char_indices().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut line_leading = true;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];

        if in_string {
            if let Some(next_i) = advance_single_quote(&chars, i, ch, &mut in_string) {
                i = next_i;
                continue;
            }
            if ch == '\n' {
                line_leading = true;
            }
            i += 1;
            continue;
        }

        if ch == '\n' {
            line_leading = true;
            i += 1;
            continue;
        }
        if line_leading && ch.is_whitespace() {
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
        if let Some(next_i) = advance_single_quote(&chars, i, ch, &mut in_string) {
            line_leading = false;
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_double_quoted_identifier(&chars, i, ch) {
            line_leading = false;
            i = next_i;
            continue;
        }
        if let Some(next_i) = skip_dollar_quoted_string(sql, &chars, i, byte_pos, ch) {
            line_leading = false;
            i = next_i;
            continue;
        }

        if line_leading && matches!(ch, '.' | '\\') {
            return true;
        }

        line_leading = false;
        i += 1;
    }

    false
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

fn low_immediate() -> SqlRiskDecision {
    SqlRiskDecision {
        risk_level: RiskLevel::Low,
        confirmation: ConfirmationType::Immediate,
    }
}

// Fallback gate for statements whose risk is high but whose target name could
// not be extracted (e.g. `DROP TABLE a, b`); Immediate here would skip
// confirmation entirely.
fn high_acknowledge(kind: &StatementKind) -> SqlRiskDecision {
    SqlRiskDecision {
        risk_level: RiskLevel::High,
        confirmation: ConfirmationType::Acknowledge {
            reason: AcknowledgeReason::TargetNameUnavailable,
            label: write_guardrails::evaluate_sql_risk(kind).label.to_string(),
        },
    }
}

pub fn evaluate_sql_risk(kind: &StatementKind, sql: &str) -> SqlRiskDecision {
    match kind {
        StatementKind::Select
        | StatementKind::Transaction
        | StatementKind::Insert
        | StatementKind::Create => low_immediate(),
        StatementKind::Unsupported | StatementKind::Other => {
            // Empty / comment-only input has nothing to execute; gating it
            // would show a confirm dialog for a no-op.
            if sql.trim().is_empty() || is_comment_only(sql) {
                return low_immediate();
            }
            SqlRiskDecision {
                risk_level: RiskLevel::Low,
                confirmation: ConfirmationType::Acknowledge {
                    reason: AcknowledgeReason::UnknownRisk,
                    label: first_keyword(sql).unwrap_or_else(|| "SQL".to_string()),
                },
            }
        }
        StatementKind::Update { has_where: true }
        | StatementKind::Delete { has_where: true }
        | StatementKind::Alter => SqlRiskDecision {
            risk_level: RiskLevel::Medium,
            confirmation: ConfirmationType::Immediate,
        },
        StatementKind::Drop => {
            if matches!(drop_subtype(sql).as_deref(), Some("table" | "database")) {
                match extract_target_name(sql, kind) {
                    Some(name) => SqlRiskDecision {
                        risk_level: RiskLevel::High,
                        confirmation: ConfirmationType::TableNameInput { target: name },
                    },
                    None => high_acknowledge(kind),
                }
            } else {
                low_immediate()
            }
        }
        StatementKind::Update { has_where: false }
        | StatementKind::Delete { has_where: false }
        | StatementKind::Truncate => match extract_target_name(sql, kind) {
            Some(name) => SqlRiskDecision {
                risk_level: RiskLevel::High,
                confirmation: ConfirmationType::TableNameInput { target: name },
            },
            None => high_acknowledge(kind),
        },
    }
}

pub fn evaluate_multi_statement(sql: &str) -> MultiStatementDecision {
    if contains_cli_meta_command(sql) {
        return MultiStatementDecision::Block {
            reason: "CLI meta-commands are not supported in SQL input".to_string(),
        };
    }

    let statements = split_statements(sql);

    if statements.is_empty() {
        return MultiStatementDecision::Block {
            reason: "Empty input".to_string(),
        };
    }

    let mut decisions: Vec<(String, SqlRiskDecision)> = Vec::new();

    for stmt in &statements {
        let kind = classify(stmt);
        let decision = evaluate_sql_risk(&kind, stmt);
        decisions.push((stmt.clone(), decision));
    }

    let has_table_name_input = decisions
        .iter()
        .any(|(_, d)| matches!(d.confirmation, ConfirmationType::TableNameInput { .. }));
    let ack_reasons: Vec<&AcknowledgeReason> = decisions
        .iter()
        .filter_map(|(_, d)| match &d.confirmation {
            ConfirmationType::Acknowledge { reason, .. } => Some(reason),
            _ => None,
        })
        .collect();
    let has_acknowledge = !ack_reasons.is_empty();
    let mixed_ack_reasons = ack_reasons.windows(2).any(|w| w[0] != w[1]);

    // One dialog can only carry one consent: a typed-name confirmation must not
    // silently approve statements that need their own acknowledgment, and one
    // acknowledgment must not cover statements flagged for a different reason
    // (the dialog would hide the other reason from the user).
    if (has_table_name_input && has_acknowledge) || mixed_ack_reasons {
        return MultiStatementDecision::Block {
            reason: "Statements require different confirmations; run them separately".to_string(),
        };
    }

    let max_risk = decisions.iter().map(|(_, d)| d.risk_level).max().unwrap();
    let confirmation = if has_table_name_input {
        decisions
            .iter()
            .find(|(_, d)| matches!(d.confirmation, ConfirmationType::TableNameInput { .. }))
            .map(|(_, d)| d.confirmation.clone())
            .unwrap()
    } else if has_acknowledge {
        // Mixed reasons are blocked above; the first Acknowledge represents all.
        decisions
            .iter()
            .find(|(_, d)| matches!(d.confirmation, ConfirmationType::Acknowledge { .. }))
            .map(|(_, d)| d.confirmation.clone())
            .unwrap()
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
        #[case::select(StatementKind::Select, "SELECT 1", RiskLevel::Low)]
        #[case::transaction(StatementKind::Transaction, "BEGIN", RiskLevel::Low)]
        #[case::insert(StatementKind::Insert, "INSERT INTO users VALUES (1)", RiskLevel::Low)]
        #[case::create(StatementKind::Create, "CREATE TABLE t (id INT)", RiskLevel::Low)]
        fn low_risk_returns_immediate(
            #[case] kind: StatementKind,
            #[case] sql: &str,
            #[case] expected_risk: RiskLevel,
        ) {
            let result = evaluate_sql_risk(&kind, sql);
            assert_eq!(result.risk_level, expected_risk);
            assert!(matches!(result.confirmation, ConfirmationType::Immediate));
        }

        #[rstest]
        #[case::grant(StatementKind::Unsupported, "GRANT SELECT ON users TO role1", "GRANT")]
        #[case::do_block(
            StatementKind::Unsupported,
            "DO $$ BEGIN DELETE FROM users; END $$",
            "DO"
        )]
        #[case::copy(StatementKind::Unsupported, "COPY users FROM '/tmp/data.csv'", "COPY")]
        #[case::select_into(StatementKind::Other, "SELECT * INTO backup FROM users", "SELECT")]
        #[case::unparseable(StatementKind::Other, "??? invalid", "INVALID")]
        fn unassessable_requires_acknowledgment(
            #[case] kind: StatementKind,
            #[case] sql: &str,
            #[case] expected_label: &str,
        ) {
            let result = evaluate_sql_risk(&kind, sql);

            assert_eq!(result.risk_level, RiskLevel::Low);
            assert!(matches!(
                result.confirmation,
                ConfirmationType::Acknowledge {
                    reason: AcknowledgeReason::UnknownRisk,
                    ref label,
                } if label == expected_label
            ));
        }

        #[rstest]
        #[case::empty("")]
        #[case::whitespace_only("   ")]
        #[case::comment_only("-- just a comment")]
        #[case::block_comment_only("/* nothing */")]
        fn empty_or_comment_only_other_returns_immediate(#[case] sql: &str) {
            let result = evaluate_sql_risk(&StatementKind::Other, sql);

            assert_eq!(result.risk_level, RiskLevel::Low);
            assert!(matches!(result.confirmation, ConfirmationType::Immediate));
        }

        #[rstest]
        #[case::drop_multiple(StatementKind::Drop, "DROP TABLE a, b", "DROP")]
        #[case::truncate_multiple(StatementKind::Truncate, "TRUNCATE a, b", "TRUNCATE")]
        fn high_without_target_requires_acknowledgment(
            #[case] kind: StatementKind,
            #[case] sql: &str,
            #[case] expected_label: &str,
        ) {
            let result = evaluate_sql_risk(&kind, sql);

            assert_eq!(result.risk_level, RiskLevel::High);
            assert!(matches!(
                result.confirmation,
                ConfirmationType::Acknowledge {
                    reason: AcknowledgeReason::TargetNameUnavailable,
                    ref label,
                } if label == expected_label
            ));
        }

        #[rstest]
        #[case::update_where(StatementKind::Update { has_where: true }, "UPDATE users SET x=1 WHERE id=1")]
        #[case::delete_where(StatementKind::Delete { has_where: true }, "DELETE FROM users WHERE id=1")]
        #[case::alter(StatementKind::Alter, "ALTER TABLE users ADD COLUMN x INT")]
        fn medium_risk_returns_immediate(#[case] kind: StatementKind, #[case] sql: &str) {
            let result = evaluate_sql_risk(&kind, sql);
            assert_eq!(result.risk_level, RiskLevel::Medium);
            assert!(matches!(result.confirmation, ConfirmationType::Immediate));
        }

        #[rstest]
        #[case::update_no_where(StatementKind::Update { has_where: false }, "UPDATE users SET x=1")]
        #[case::delete_no_where(StatementKind::Delete { has_where: false }, "DELETE FROM users")]
        #[case::drop(StatementKind::Drop, "DROP TABLE users")]
        #[case::truncate(StatementKind::Truncate, "TRUNCATE users")]
        fn high_table_name_input(#[case] kind: StatementKind, #[case] sql: &str) {
            let result = evaluate_sql_risk(&kind, sql);
            assert_eq!(result.risk_level, RiskLevel::High);
            assert!(matches!(
                result.confirmation,
                ConfirmationType::TableNameInput { .. }
            ));
        }

        #[test]
        fn drop_database_returns_high_table_name_input() {
            let result = evaluate_sql_risk(&StatementKind::Drop, "DROP DATABASE production");
            assert_eq!(result.risk_level, RiskLevel::High);
            assert!(matches!(
                result.confirmation,
                ConfirmationType::TableNameInput { .. }
            ));
        }

        #[test]
        fn drop_table_with_leading_comment_returns_high() {
            let result =
                evaluate_sql_risk(&StatementKind::Drop, "-- cleanup\nDROP TABLE production");
            assert_eq!(result.risk_level, RiskLevel::High);
            assert!(matches!(
                result.confirmation,
                ConfirmationType::TableNameInput { .. }
            ));
        }

        #[rstest]
        #[case::drop_index("DROP INDEX my_index")]
        #[case::drop_policy("DROP POLICY p ON t")]
        #[case::drop_view("DROP VIEW v")]
        #[case::drop_schema("DROP SCHEMA s")]
        #[case::drop_owned_by("DROP OWNED BY role")]
        #[case::drop_tablespace("DROP TABLESPACE fastdisk")]
        fn non_table_drop_returns_low_immediate(#[case] sql: &str) {
            let result = evaluate_sql_risk(&StatementKind::Drop, sql);
            assert_eq!(result.risk_level, RiskLevel::Low);
            assert!(matches!(result.confirmation, ConfirmationType::Immediate));
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
                    assert!(matches!(risk.confirmation, ConfirmationType::Immediate));
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
        fn tcl_only_multi_returns_immediate() {
            let result = evaluate_multi_statement("BEGIN; COMMIT");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.confirmation, ConfirmationType::Immediate);
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn multiple_high_uses_first_target() {
            let result = evaluate_multi_statement("DROP TABLE a; DROP TABLE b");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::High);
                    assert!(matches!(
                        risk.confirmation,
                        ConfirmationType::TableNameInput { ref target } if target == "a"
                    ));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn select_into_requires_acknowledgment() {
            let result = evaluate_multi_statement("SELECT * INTO backup FROM users");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(
                        risk.confirmation,
                        ConfirmationType::Acknowledge {
                            reason: AcknowledgeReason::UnknownRisk,
                            ..
                        }
                    ));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn risk_aggregation_select_insert() {
            let result = evaluate_multi_statement("SELECT 1; INSERT INTO users VALUES (1)");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(risk.confirmation, ConfirmationType::Immediate));
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

        #[rstest]
        #[case::sqlite_shell(".shell echo injected")]
        #[case::sqlite_open_after_select("SELECT 1;\n.open writable.db")]
        #[case::psql_shell("\\! echo injected")]
        #[case::indented_meta_command("  .output /tmp/out.csv")]
        fn cli_meta_commands_are_blocked(#[case] sql: &str) {
            let result = evaluate_multi_statement(sql);

            assert!(matches!(
                result,
                MultiStatementDecision::Block { reason }
                    if reason == "CLI meta-commands are not supported in SQL input"
            ));
        }

        #[rstest]
        #[case::dot_inside_string("SELECT '.shell echo ok'")]
        #[case::backslash_inside_string("SELECT '\\\\! echo ok'")]
        #[case::dot_inside_comment("-- .shell ignored\nSELECT 1")]
        #[case::dot_inside_dollar_quote("SELECT $tag$\n.shell ignored\n$tag$")]
        fn cli_meta_command_like_text_is_allowed(#[case] sql: &str) {
            let result = evaluate_multi_statement(sql);

            assert!(matches!(result, MultiStatementDecision::Allow { .. }));
        }

        #[test]
        fn do_block_requires_acknowledgment() {
            let result = evaluate_multi_statement("DO $$ BEGIN RAISE NOTICE 'hi'; END $$");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(
                        risk.confirmation,
                        ConfirmationType::Acknowledge {
                            reason: AcknowledgeReason::UnknownRisk,
                            ref label,
                        } if label == "DO"
                    ));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn copy_requires_acknowledgment() {
            let result = evaluate_multi_statement("COPY users FROM '/tmp/data.csv'");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(
                        risk.confirmation,
                        ConfirmationType::Acknowledge {
                            reason: AcknowledgeReason::UnknownRisk,
                            ref label,
                        } if label == "COPY"
                    ));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn do_block_after_select_requires_acknowledgment() {
            let result =
                evaluate_multi_statement("SELECT 1; DO $$ BEGIN DELETE FROM users; END $$");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(
                        risk.confirmation,
                        ConfirmationType::Acknowledge {
                            reason: AcknowledgeReason::UnknownRisk,
                            ..
                        }
                    ));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[rstest]
        #[case::do_block_with_drop("DO $$ BEGIN RAISE NOTICE 'hi'; END $$; DROP TABLE users")]
        #[case::drop_with_unextractable_drop("DROP TABLE users; DROP TABLE a, b")]
        #[case::mixed_acknowledge_reasons("DO $$ BEGIN RAISE NOTICE 'hi'; END $$; DROP TABLE a, b")]
        fn mixed_confirmations_are_blocked(#[case] sql: &str) {
            let result = evaluate_multi_statement(sql);

            assert!(matches!(result, MultiStatementDecision::Block { .. }));
        }

        #[rstest]
        #[case::unknown_risk(
            "COPY users FROM '/tmp/a.csv'; CALL refresh()",
            RiskLevel::Low,
            AcknowledgeReason::UnknownRisk,
            "COPY"
        )]
        #[case::target_name_unavailable(
            "DROP TABLE a, b; TRUNCATE c, d",
            RiskLevel::High,
            AcknowledgeReason::TargetNameUnavailable,
            "DROP"
        )]
        fn same_reason_acknowledgments_aggregate_with_first_label(
            #[case] sql: &str,
            #[case] expected_risk: RiskLevel,
            #[case] expected_reason: AcknowledgeReason,
            #[case] expected_label: &str,
        ) {
            let result = evaluate_multi_statement(sql);

            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, expected_risk);
                    assert!(matches!(
                        risk.confirmation,
                        ConfirmationType::Acknowledge {
                            ref reason,
                            ref label,
                        } if *reason == expected_reason && label == expected_label
                    ));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn drop_multiple_targets_requires_acknowledgment() {
            let result = evaluate_multi_statement("DROP TABLE a, b");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::High);
                    assert!(matches!(
                        risk.confirmation,
                        ConfirmationType::Acknowledge {
                            reason: AcknowledgeReason::TargetNameUnavailable,
                            ..
                        }
                    ));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn insert_then_select_returns_immediate() {
            let result = evaluate_multi_statement("INSERT INTO users VALUES (1); SELECT 1");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(risk.confirmation, ConfirmationType::Immediate));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn drop_index_returns_low_immediate() {
            let result = evaluate_multi_statement("DROP INDEX my_index");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(risk.confirmation, ConfirmationType::Immediate));
                }
                _ => panic!("expected Allow"),
            }
        }

        #[test]
        fn drop_owned_by_returns_low_immediate() {
            let result = evaluate_multi_statement("DROP OWNED BY role");
            match result {
                MultiStatementDecision::Allow { risk, .. } => {
                    assert_eq!(risk.risk_level, RiskLevel::Low);
                    assert!(matches!(risk.confirmation, ConfirmationType::Immediate));
                }
                _ => panic!("expected Allow"),
            }
        }
    }
}
