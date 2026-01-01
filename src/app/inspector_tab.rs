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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_wraps_from_last_to_first() {
        let tab = InspectorTab::Ddl;
        let result = tab.next();
        assert_eq!(result, InspectorTab::Columns);
    }

    #[test]
    fn prev_wraps_from_first_to_last() {
        let tab = InspectorTab::Columns;
        let result = tab.prev();
        assert_eq!(result, InspectorTab::Ddl);
    }

    #[test]
    fn next_cycles_through_all_tabs() {
        let mut tab = InspectorTab::Columns;
        tab = tab.next();
        assert_eq!(tab, InspectorTab::Indexes);
        tab = tab.next();
        assert_eq!(tab, InspectorTab::ForeignKeys);
        tab = tab.next();
        assert_eq!(tab, InspectorTab::Rls);
        tab = tab.next();
        assert_eq!(tab, InspectorTab::Ddl);
        tab = tab.next();
        assert_eq!(tab, InspectorTab::Columns);
    }

    #[test]
    fn prev_cycles_through_all_tabs_backward() {
        let mut tab = InspectorTab::Columns;
        tab = tab.prev();
        assert_eq!(tab, InspectorTab::Ddl);
        tab = tab.prev();
        assert_eq!(tab, InspectorTab::Rls);
        tab = tab.prev();
        assert_eq!(tab, InspectorTab::ForeignKeys);
        tab = tab.prev();
        assert_eq!(tab, InspectorTab::Indexes);
        tab = tab.prev();
        assert_eq!(tab, InspectorTab::Columns);
    }
}
