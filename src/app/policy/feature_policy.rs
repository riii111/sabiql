use crate::model::shared::engine_feature_profile::EngineFeatureProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureRequirement {
    None,
    ErDiagram,
    JsonbDetail,
    SqliteDiagnostics,
    Explain,
    ExplainAnalyze,
    PlanComparison,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureAvailability {
    Hidden,
    Enabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeaturePolicy {
    profile: EngineFeatureProfile,
}

impl FeaturePolicy {
    pub fn new(profile: &EngineFeatureProfile) -> Self {
        Self { profile: *profile }
    }

    pub fn availability(&self, requirement: FeatureRequirement) -> FeatureAvailability {
        let supported = match requirement {
            FeatureRequirement::None => true,
            FeatureRequirement::ErDiagram => self.profile.supports_er_diagram(),
            FeatureRequirement::JsonbDetail => self.profile.supports_jsonb_detail(),
            FeatureRequirement::SqliteDiagnostics => self.profile.supports_sqlite_diagnostics(),
            FeatureRequirement::Explain => self.profile.supports_explain(),
            FeatureRequirement::ExplainAnalyze => self.profile.supports_explain_analyze(),
            FeatureRequirement::PlanComparison => self.profile.supports_plan_comparison(),
        };

        if supported {
            FeatureAvailability::Enabled
        } else {
            FeatureAvailability::Hidden
        }
    }

    pub fn is_visible(&self, requirement: FeatureRequirement) -> bool {
        !matches!(self.availability(requirement), FeatureAvailability::Hidden)
    }

    pub fn is_enabled(&self, requirement: FeatureRequirement) -> bool {
        matches!(self.availability(requirement), FeatureAvailability::Enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_profile_enables_postgres_features_and_hides_sqlite_diagnostics() {
        let policy = FeaturePolicy::new(&EngineFeatureProfile::postgres_like());

        assert_eq!(
            policy.availability(FeatureRequirement::ErDiagram),
            FeatureAvailability::Enabled
        );
        assert_eq!(
            policy.availability(FeatureRequirement::JsonbDetail),
            FeatureAvailability::Enabled
        );
        assert_eq!(
            policy.availability(FeatureRequirement::ExplainAnalyze),
            FeatureAvailability::Enabled
        );
        assert_eq!(
            policy.availability(FeatureRequirement::SqliteDiagnostics),
            FeatureAvailability::Hidden
        );
    }

    #[test]
    fn sqlite_profile_enables_diagnostics_and_hides_postgres_features() {
        let policy = FeaturePolicy::new(&EngineFeatureProfile::sqlite_like());

        assert_eq!(
            policy.availability(FeatureRequirement::SqliteDiagnostics),
            FeatureAvailability::Enabled
        );
        assert_eq!(
            policy.availability(FeatureRequirement::ErDiagram),
            FeatureAvailability::Hidden
        );
        assert_eq!(
            policy.availability(FeatureRequirement::JsonbDetail),
            FeatureAvailability::Hidden
        );
        assert_eq!(
            policy.availability(FeatureRequirement::PlanComparison),
            FeatureAvailability::Hidden
        );
    }

    #[test]
    fn unrequired_operations_are_enabled() {
        let policy = FeaturePolicy::new(&EngineFeatureProfile::disconnected());

        assert!(policy.is_visible(FeatureRequirement::None));
        assert!(policy.is_enabled(FeatureRequirement::None));
    }
}
