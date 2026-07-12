use crate::domain::{DatabaseType, QueryResult, QueryValue, Table, TableKind};
use crate::policy::sql::statement_classifier::StatementKind;
use crate::policy::write::write_update::build_pk_pairs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StableRowIdentity {
    PrimaryKey(Vec<String>),
    SqliteRowid { alias: String },
}

impl StableRowIdentity {
    pub fn identity_pairs_for_row(
        &self,
        result: &QueryResult,
        row_idx: usize,
    ) -> Option<Vec<(String, QueryValue)>> {
        match self {
            Self::PrimaryKey(columns) => {
                let row = result.values().get(row_idx)?;
                build_pk_pairs(&result.columns, row, columns)
            }
            Self::SqliteRowid { alias } => result
                .hidden_value_at(row_idx, alias)
                .cloned()
                .map(|value| vec![(alias.clone(), value)]),
        }
    }

    pub fn predicate_pairs_for_row(
        &self,
        result: &QueryResult,
        row_idx: usize,
    ) -> Option<Vec<(String, QueryValue)>> {
        match self {
            Self::PrimaryKey(_) => self.identity_pairs_for_row(result, row_idx),
            Self::SqliteRowid { alias } => {
                let rowid = result.hidden_value_at(row_idx, alias)?.clone();
                let row = result.values().get(row_idx)?;
                if row.is_empty() || result.columns.is_empty() || row.len() != result.columns.len()
                {
                    return None;
                }
                let mut pairs = Vec::with_capacity(row.len() + 1);
                pairs.push((alias.clone(), rowid));
                pairs.extend(result.columns.iter().cloned().zip(row.iter().cloned()));
                Some(pairs)
            }
        }
    }

    pub fn is_primary_key_column(&self, column_name: &str) -> bool {
        match self {
            Self::PrimaryKey(columns) => columns.iter().any(|pk| pk == column_name),
            Self::SqliteRowid { alias: _ } => false,
        }
    }

    pub fn uses_sqlite_rowid(&self) -> bool {
        matches!(self, Self::SqliteRowid { .. })
    }
}

/// Resolves the stable identity used to target a row in a write preview.
///
/// Callers must validate `preview_writeability` first. This function only
/// resolves primary-key or SQLite rowid identity; it does not enforce whether
/// the table itself is writable.
pub fn stable_row_identity_for_table(
    database_type: DatabaseType,
    table: &Table,
) -> Option<StableRowIdentity> {
    if let Some(pk) = table.primary_key.as_ref().filter(|cols| !cols.is_empty()) {
        return Some(StableRowIdentity::PrimaryKey(pk.clone()));
    }
    if database_type != DatabaseType::SQLite {
        return None;
    }
    table
        .sqlite_rowid_alias()
        .map(|alias| StableRowIdentity::SqliteRowid {
            alias: alias.to_string(),
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewWriteability {
    Writable,
    ReadOnly(&'static str),
    MissingStableRowIdentity,
}

pub fn preview_writeability(database_type: DatabaseType, table: &Table) -> PreviewWriteability {
    if table.kind_info.kind == TableKind::View {
        return PreviewWriteability::ReadOnly("view");
    }
    if table.kind_info.kind == TableKind::Virtual {
        return PreviewWriteability::ReadOnly("virtual table");
    }
    if table.kind_info.without_rowid {
        return PreviewWriteability::ReadOnly("WITHOUT ROWID table");
    }
    let has_primary_key = table
        .primary_key
        .as_ref()
        .is_some_and(|columns| !columns.is_empty());
    let has_sqlite_rowid =
        database_type == DatabaseType::SQLite && table.sqlite_rowid_alias().is_some();
    if !has_primary_key && !has_sqlite_rowid {
        return PreviewWriteability::MissingStableRowIdentity;
    }
    PreviewWriteability::Writable
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOperation {
    Update,
    Delete,
}

// Variant order matters: derives `Ord` for risk comparison (Low < Medium < High).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSummary {
    pub schema: String,
    pub table: String,
    pub key_values: Vec<(String, QueryValue)>,
    pub uses_sqlite_rowid: bool,
}

impl TargetSummary {
    pub fn identity_label(&self) -> &'static str {
        if self.uses_sqlite_rowid {
            "SQLite rowid"
        } else {
            "Primary key"
        }
    }

    pub fn format_compact(&self) -> String {
        let key_str = self
            .key_values
            .iter()
            .map(|(k, v)| format!("{k}={}", v.display_value()))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}.{} ({})", self.schema, self.table, key_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailDecision {
    pub risk_level: RiskLevel,
    pub blocked: bool,
    pub reason: Option<String>,
    pub target_summary: Option<TargetSummary>,
}

use crate::policy::json::json_diff::JsonDiffLine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDiff {
    pub column: String,
    pub before: String,
    pub after: String,
    pub json_diff: Option<Vec<JsonDiffLine>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritePreview {
    pub operation: WriteOperation,
    pub sql: String,
    pub target_summary: TargetSummary,
    pub diff: Vec<ColumnDiff>,
    pub guardrail: GuardrailDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdhocRiskDecision {
    pub risk_level: RiskLevel,
    // All values are string literals, so `&'static str` avoids allocation and keeps `Copy`.
    pub label: &'static str,
}

pub fn evaluate_sql_risk(kind: &StatementKind) -> AdhocRiskDecision {
    let (risk_level, label) = match kind {
        StatementKind::Insert => (RiskLevel::Low, "INSERT"),
        StatementKind::Create => (RiskLevel::Low, "CREATE"),
        StatementKind::Update { has_where: true } => (RiskLevel::Medium, "UPDATE"),
        StatementKind::Delete { has_where: true } => (RiskLevel::Medium, "DELETE"),
        StatementKind::Alter => (RiskLevel::Medium, "ALTER"),
        StatementKind::Update { has_where: false } => (RiskLevel::High, "UPDATE (no WHERE)"),
        StatementKind::Delete { has_where: false } => (RiskLevel::High, "DELETE (no WHERE)"),
        StatementKind::Drop => (RiskLevel::High, "DROP"),
        StatementKind::Truncate => (RiskLevel::High, "TRUNCATE"),
        StatementKind::Unsupported
        | StatementKind::Other
        | StatementKind::Select
        | StatementKind::Transaction => (RiskLevel::Low, "SQL"),
    };
    AdhocRiskDecision { risk_level, label }
}

pub fn evaluate_guardrails(
    has_where: bool,
    has_stable_row_identity: bool,
    target_summary: Option<TargetSummary>,
) -> GuardrailDecision {
    if !has_where {
        return GuardrailDecision {
            risk_level: RiskLevel::High,
            blocked: true,
            reason: Some("WHERE clause is missing".to_string()),
            target_summary,
        };
    }

    if !has_stable_row_identity {
        return GuardrailDecision {
            risk_level: RiskLevel::High,
            blocked: true,
            reason: Some("Stable row identity is missing".to_string()),
            target_summary,
        };
    }

    GuardrailDecision {
        risk_level: RiskLevel::Low,
        blocked: false,
        reason: None,
        target_summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TableKindInfo;
    use crate::test_support;

    mod guardrail_evaluation {
        use super::*;

        #[test]
        fn missing_where_returns_blocked_high_risk() {
            let decision = evaluate_guardrails(false, true, None);
            assert_eq!(decision.risk_level, RiskLevel::High);
            assert!(decision.blocked);
        }

        #[test]
        fn missing_stable_identity_returns_blocked_high_risk() {
            let decision = evaluate_guardrails(true, false, None);
            assert_eq!(decision.risk_level, RiskLevel::High);
            assert!(decision.blocked);
        }

        #[test]
        fn stable_where_and_identity_returns_unblocked_low_risk() {
            let target = TargetSummary {
                schema: "public".to_string(),
                table: "users".to_string(),
                key_values: vec![("id".to_string(), QueryValue::text("42"))],
                uses_sqlite_rowid: false,
            };
            let decision = evaluate_guardrails(true, true, Some(target));
            assert_eq!(decision.risk_level, RiskLevel::Low);
            assert!(!decision.blocked);
        }

        #[test]
        fn target_summary_with_single_key_returns_compact_format() {
            let target = TargetSummary {
                schema: "public".to_string(),
                table: "users".to_string(),
                key_values: vec![("id".to_string(), QueryValue::text("42"))],
                uses_sqlite_rowid: false,
            };
            assert_eq!(target.format_compact(), "public.users (id=42)");
        }
    }

    mod row_identity {
        use super::*;

        fn primary_key_table() -> Table {
            let mut table = test_support::table::minimal("public", "users");
            table.primary_key = Some(vec!["id".to_string()]);
            table
        }

        #[test]
        fn postgres_primary_key_uses_primary_key_identity() {
            let table = primary_key_table();

            assert_eq!(
                preview_writeability(DatabaseType::PostgreSQL, &table),
                PreviewWriteability::Writable
            );
            assert_eq!(
                stable_row_identity_for_table(DatabaseType::PostgreSQL, &table),
                Some(StableRowIdentity::PrimaryKey(vec!["id".to_string()]))
            );
        }

        #[test]
        fn sqlite_rowid_table_uses_rowid_identity() {
            let table = test_support::table::minimal("main", "users");

            assert_eq!(
                preview_writeability(DatabaseType::SQLite, &table),
                PreviewWriteability::Writable
            );
            assert_eq!(
                stable_row_identity_for_table(DatabaseType::SQLite, &table),
                Some(StableRowIdentity::SqliteRowid {
                    alias: "rowid".to_string()
                })
            );
        }

        #[test]
        fn postgres_table_without_primary_key_has_no_stable_identity() {
            let table = test_support::table::minimal("public", "users");

            assert_eq!(
                preview_writeability(DatabaseType::PostgreSQL, &table),
                PreviewWriteability::MissingStableRowIdentity
            );
            assert_eq!(
                stable_row_identity_for_table(DatabaseType::PostgreSQL, &table),
                None
            );
        }

        #[test]
        fn readonly_table_kinds_are_not_writable() {
            let cases = [
                (TableKind::View, false, "view"),
                (TableKind::Virtual, false, "virtual table"),
                (TableKind::Table, true, "WITHOUT ROWID table"),
            ];

            for (kind, without_rowid, reason) in cases {
                let mut table = primary_key_table();
                table.kind_info = TableKindInfo {
                    kind,
                    without_rowid,
                    ..TableKindInfo::default()
                };

                assert_eq!(
                    preview_writeability(DatabaseType::SQLite, &table),
                    PreviewWriteability::ReadOnly(reason)
                );
                assert_eq!(
                    stable_row_identity_for_table(DatabaseType::SQLite, &table),
                    Some(StableRowIdentity::PrimaryKey(vec!["id".to_string()]))
                );
            }
        }
    }

    mod adhoc_risk {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case(StatementKind::Insert, RiskLevel::Low, "INSERT")]
        #[case(StatementKind::Create, RiskLevel::Low, "CREATE")]
        #[case(StatementKind::Update { has_where: true }, RiskLevel::Medium, "UPDATE")]
        #[case(StatementKind::Delete { has_where: true }, RiskLevel::Medium, "DELETE")]
        #[case(StatementKind::Alter, RiskLevel::Medium, "ALTER")]
        #[case(StatementKind::Update { has_where: false }, RiskLevel::High, "UPDATE (no WHERE)")]
        #[case(StatementKind::Delete { has_where: false }, RiskLevel::High, "DELETE (no WHERE)")]
        #[case(StatementKind::Drop, RiskLevel::High, "DROP")]
        #[case(StatementKind::Truncate, RiskLevel::High, "TRUNCATE")]
        #[case(StatementKind::Other, RiskLevel::Low, "SQL")]
        #[case(StatementKind::Unsupported, RiskLevel::Low, "SQL")]
        fn risk_level_and_label(
            #[case] kind: StatementKind,
            #[case] expected_risk: RiskLevel,
            #[case] expected_label: &str,
        ) {
            let decision = evaluate_sql_risk(&kind);
            assert_eq!(decision.risk_level, expected_risk);
            assert_eq!(decision.label, expected_label);
        }
    }
}
