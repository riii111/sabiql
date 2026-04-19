#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorFeature {
    Info,
    Columns,
    Indexes,
    ForeignKeys,
    Rls,
    Triggers,
    Ddl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseCapabilities {
    pub supports_explain: bool,
    pub supported_inspector_features: Vec<InspectorFeature>,
}

impl DatabaseCapabilities {
    pub fn new(
        supports_explain: bool,
        supported_inspector_features: Vec<InspectorFeature>,
    ) -> Self {
        Self {
            supports_explain,
            supported_inspector_features,
        }
    }
}

pub trait DatabaseCapabilityProvider {
    fn capabilities(&self) -> DatabaseCapabilities;
}
