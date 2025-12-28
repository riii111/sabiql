/// Represents the active sub-tab in the Inspector pane
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Columns,
    Indexes,
    ForeignKeys,
    Rls,
    Ddl,
}

#[allow(dead_code)]
impl InspectorTab {
    /// Get the next tab in order
    pub fn next(self) -> Self {
        match self {
            Self::Columns => Self::Indexes,
            Self::Indexes => Self::ForeignKeys,
            Self::ForeignKeys => Self::Rls,
            Self::Rls => Self::Ddl,
            Self::Ddl => Self::Columns,
        }
    }

    /// Get the previous tab in order
    pub fn prev(self) -> Self {
        match self {
            Self::Columns => Self::Ddl,
            Self::Indexes => Self::Columns,
            Self::ForeignKeys => Self::Indexes,
            Self::Rls => Self::ForeignKeys,
            Self::Ddl => Self::Rls,
        }
    }

    /// Get the display name for the tab
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Columns => "Cols",
            Self::Indexes => "Idx",
            Self::ForeignKeys => "FK",
            Self::Rls => "RLS",
            Self::Ddl => "DDL",
        }
    }

    /// Get all tabs in order
    pub fn all() -> &'static [Self] {
        &[
            Self::Columns,
            Self::Indexes,
            Self::ForeignKeys,
            Self::Rls,
            Self::Ddl,
        ]
    }

    /// Get the tab index (0-based)
    pub fn index(self) -> usize {
        match self {
            Self::Columns => 0,
            Self::Indexes => 1,
            Self::ForeignKeys => 2,
            Self::Rls => 3,
            Self::Ddl => 4,
        }
    }

    /// Create a tab from index (1-based, for keyboard shortcuts)
    pub fn from_key(key: char) -> Option<Self> {
        match key {
            '1' => Some(Self::Columns),
            '2' => Some(Self::Indexes),
            '3' => Some(Self::ForeignKeys),
            '4' => Some(Self::Rls),
            '5' => Some(Self::Ddl),
            _ => None,
        }
    }
}
