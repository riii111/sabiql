use crate::model::shared::engine_feature_profile::EngineFeatureProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureRequirement {
    None,
    ErDiagram,
    JsonDocumentDetail,
    JsonDocumentEdit,
    SqliteDiagnostics,
    Explain,
    ExplainAnalyze,
    PlanComparison,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeaturePolicy {
    profile: EngineFeatureProfile,
}

impl FeaturePolicy {
    pub fn new(profile: &EngineFeatureProfile) -> Self {
        Self { profile: *profile }
    }

    pub fn is_enabled(&self, requirement: FeatureRequirement) -> bool {
        match requirement {
            FeatureRequirement::None => true,
            FeatureRequirement::ErDiagram => self.profile.supports_er_diagram(),
            FeatureRequirement::JsonDocumentDetail => self.profile.supports_json_document_detail(),
            FeatureRequirement::JsonDocumentEdit => self.profile.supports_json_document_edit(),
            FeatureRequirement::SqliteDiagnostics => self.profile.supports_sqlite_diagnostics(),
            FeatureRequirement::Explain => self.profile.supports_explain(),
            FeatureRequirement::ExplainAnalyze => self.profile.supports_explain_analyze(),
            FeatureRequirement::PlanComparison => self.profile.supports_plan_comparison(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_requirements_match_each_engine_profile() {
        let requirements = [
            FeatureRequirement::None,
            FeatureRequirement::ErDiagram,
            FeatureRequirement::JsonDocumentDetail,
            FeatureRequirement::JsonDocumentEdit,
            FeatureRequirement::SqliteDiagnostics,
            FeatureRequirement::Explain,
            FeatureRequirement::ExplainAnalyze,
            FeatureRequirement::PlanComparison,
        ];
        let profiles = [
            (
                "postgresql",
                EngineFeatureProfile::postgres_like(),
                [true, true, true, true, false, true, true, true],
            ),
            (
                "sqlite",
                EngineFeatureProfile::sqlite_like(),
                [true, false, false, false, true, true, false, false],
            ),
            (
                "mysql",
                EngineFeatureProfile::mysql_like(),
                [true, true, true, true, false, true, true, true],
            ),
            (
                "disconnected",
                EngineFeatureProfile::disconnected(),
                [true, false, false, false, false, false, false, false],
            ),
        ];

        for (name, profile, expected) in profiles {
            let policy = FeaturePolicy::new(&profile);

            for (requirement, expected) in requirements.iter().zip(expected) {
                assert_eq!(
                    policy.is_enabled(*requirement),
                    expected,
                    "{name}: {requirement:?}"
                );
            }
        }
    }
}
