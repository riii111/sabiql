#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOperation {
    Update,
    /// Reserved for future write-preview flows (e.g. issue #107).
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub key_values: Vec<(String, String)>,
}

impl TargetSummary {
    pub fn format_compact(&self) -> String {
        let key_str = self
            .key_values
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDiff {
    pub column: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritePreview {
    pub operation: WriteOperation,
    pub sql: String,
    pub target_summary: TargetSummary,
    pub diff: Vec<ColumnDiff>,
    pub guardrail: GuardrailDecision,
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
                key_values: vec![("id".to_string(), "42".to_string())],
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
                key_values: vec![("id".to_string(), "42".to_string())],
            };
            assert_eq!(target.format_compact(), "public.users (id=42)");
        }
    }
}
