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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorInfoField {
    Owner,
    Comment,
    RowCount,
    Schema,
    TableName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseCapabilities {
    supports_explain: bool,
    supported_inspector_features: Vec<InspectorFeature>,
    supported_inspector_info_fields: Vec<InspectorInfoField>,
}

impl DatabaseCapabilities {
    pub fn new(
        supports_explain: bool,
        supported_inspector_features: Vec<InspectorFeature>,
        supported_inspector_info_fields: Vec<InspectorInfoField>,
    ) -> Self {
        assert!(
            !supported_inspector_features.is_empty(),
            "DatabaseCapabilities requires at least one supported inspector feature"
        );
        assert!(
            !supported_inspector_info_fields.is_empty(),
            "DatabaseCapabilities requires at least one supported inspector info field"
        );
        Self {
            supports_explain,
            supported_inspector_features,
            supported_inspector_info_fields,
        }
    }

    pub fn supports_explain(&self) -> bool {
        self.supports_explain
    }

    pub fn supported_inspector_features(&self) -> &[InspectorFeature] {
        &self.supported_inspector_features
    }

    pub fn supported_inspector_info_fields(&self) -> &[InspectorInfoField] {
        &self.supported_inspector_info_fields
    }
}
