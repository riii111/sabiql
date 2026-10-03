#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Info,
    Columns,
    Indexes,
    ForeignKeys,
    Rls,
    Triggers,
    Ddl,
}

impl InspectorTab {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Columns => "Cols",
            Self::Indexes => "Idx",
            Self::ForeignKeys => "FK",
            Self::Rls => "RLS",
            Self::Triggers => "Trig",
            Self::Ddl => "DDL",
        }
    }
}
