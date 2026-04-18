use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::sql_editor::modal::SqlModalTab;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCapabilities {
    pub supports_explain: bool,
    supported_inspector_tabs: Vec<InspectorTab>,
}

impl DbCapabilities {
    pub fn supported_inspector_tabs(&self) -> &[InspectorTab] {
        &self.supported_inspector_tabs
    }

    pub fn new(supports_explain: bool, supported_inspector_tabs: Vec<InspectorTab>) -> Self {
        Self {
            supports_explain,
            supported_inspector_tabs,
        }
    }

    pub fn supports_inspector_tab(&self, tab: InspectorTab) -> bool {
        self.supported_inspector_tabs.contains(&tab)
    }

    pub fn supported_sql_modal_tabs(&self) -> &'static [SqlModalTab] {
        if self.supports_explain {
            &[
                SqlModalTab::Sql,
                SqlModalTab::Plan,
                SqlModalTab::Compare,
            ]
        } else {
            &[SqlModalTab::Sql]
        }
    }

    pub fn normalize_sql_modal_tab(&self, tab: SqlModalTab) -> SqlModalTab {
        if self.supported_sql_modal_tabs().contains(&tab) {
            tab
        } else {
            SqlModalTab::Sql
        }
    }

    pub fn next_sql_modal_tab(&self, current: SqlModalTab) -> SqlModalTab {
        self.cycle_sql_modal_tab(current, 1)
    }

    pub fn prev_sql_modal_tab(&self, current: SqlModalTab) -> SqlModalTab {
        self.cycle_sql_modal_tab(current, -1)
    }

    pub fn normalize_inspector_tab(&self, tab: InspectorTab) -> InspectorTab {
        if self.supports_inspector_tab(tab) {
            tab
        } else {
            self.supported_inspector_tabs
                .first()
                .copied()
                .unwrap_or_default()
        }
    }

    pub fn next_inspector_tab(&self, current: InspectorTab) -> InspectorTab {
        self.cycle_inspector_tab(current, 1)
    }

    pub fn prev_inspector_tab(&self, current: InspectorTab) -> InspectorTab {
        self.cycle_inspector_tab(current, -1)
    }

    fn cycle_inspector_tab(&self, current: InspectorTab, delta: isize) -> InspectorTab {
        let tabs = &self.supported_inspector_tabs;
        if tabs.is_empty() {
            return current;
        }

        let current_idx = tabs.iter().position(|tab| *tab == current).unwrap_or(0) as isize;
        let next_idx = (current_idx + delta).rem_euclid(tabs.len() as isize) as usize;
        tabs[next_idx]
    }

    fn cycle_sql_modal_tab(&self, current: SqlModalTab, delta: isize) -> SqlModalTab {
        let tabs = self.supported_sql_modal_tabs();
        let current = self.normalize_sql_modal_tab(current);
        let current_idx = tabs.iter().position(|tab| *tab == current).unwrap_or(0) as isize;
        let next_idx = (current_idx + delta).rem_euclid(tabs.len() as isize) as usize;
        tabs[next_idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_supports_all_inspector_tabs() {
        let caps = DbCapabilities::new(
            true,
            vec![
                InspectorTab::Info,
                InspectorTab::Columns,
                InspectorTab::Indexes,
                InspectorTab::ForeignKeys,
                InspectorTab::Rls,
                InspectorTab::Triggers,
                InspectorTab::Ddl,
            ],
        );

        assert!(caps.supports_explain);
        assert!(caps.supports_inspector_tab(InspectorTab::Ddl));
        assert_eq!(caps.supported_inspector_tabs().len(), 7);
    }

    #[test]
    fn normalize_unsupported_tab_returns_first_supported_tab() {
        let caps = DbCapabilities {
            supports_explain: false,
            supported_inspector_tabs: vec![InspectorTab::Info, InspectorTab::Columns],
        };

        assert_eq!(
            caps.normalize_inspector_tab(InspectorTab::Triggers),
            InspectorTab::Info
        );
    }

    #[test]
    fn normalize_supported_sql_modal_tab_passes_through() {
        let caps = DbCapabilities::new(true, vec![InspectorTab::Info]);

        assert_eq!(
            caps.normalize_sql_modal_tab(SqlModalTab::Compare),
            SqlModalTab::Compare
        );
    }

    #[test]
    fn normalize_unsupported_sql_modal_tab_returns_sql() {
        let no_explain_caps = DbCapabilities::new(false, vec![InspectorTab::Info]);
        assert_eq!(
            no_explain_caps.normalize_sql_modal_tab(SqlModalTab::Plan),
            SqlModalTab::Sql
        );
    }
}
