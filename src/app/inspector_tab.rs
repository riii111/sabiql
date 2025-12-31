#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Columns,
    Indexes,
    ForeignKeys,
    Rls,
    Ddl,
}

impl InspectorTab {
    pub fn next(self) -> Self {
        match self {
            Self::Columns => Self::Indexes,
            Self::Indexes => Self::ForeignKeys,
            Self::ForeignKeys => Self::Rls,
            Self::Rls => Self::Ddl,
            Self::Ddl => Self::Columns,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Columns => Self::Ddl,
            Self::Indexes => Self::Columns,
            Self::ForeignKeys => Self::Indexes,
            Self::Rls => Self::ForeignKeys,
            Self::Ddl => Self::Rls,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Columns => "Cols",
            Self::Indexes => "Idx",
            Self::ForeignKeys => "FK",
            Self::Rls => "RLS",
            Self::Ddl => "DDL",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Columns,
            Self::Indexes,
            Self::ForeignKeys,
            Self::Rls,
            Self::Ddl,
        ]
    }
}
