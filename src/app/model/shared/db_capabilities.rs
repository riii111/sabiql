use crate::app::model::shared::inspector_tab::InspectorTab;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCapabilities {
    pub supports_explain: bool,
    pub supports_pg_service_entries: bool,
    supported_inspector_tabs: Vec<InspectorTab>,
}

impl DbCapabilities {
    pub fn postgres() -> Self {
        Self {
            supports_explain: true,
            supports_pg_service_entries: true,
            supported_inspector_tabs: vec![
                InspectorTab::Info,
                InspectorTab::Columns,
                InspectorTab::Indexes,
                InspectorTab::ForeignKeys,
                InspectorTab::Rls,
                InspectorTab::Triggers,
                InspectorTab::Ddl,
            ],
        }
    }

    pub fn supported_inspector_tabs(&self) -> &[InspectorTab] {
        &self.supported_inspector_tabs
    }

    #[cfg(test)]
    pub fn new_for_tests(
        supports_explain: bool,
        supports_pg_service_entries: bool,
        supported_inspector_tabs: Vec<InspectorTab>,
    ) -> Self {
        Self {
            supports_explain,
            supports_pg_service_entries,
            supported_inspector_tabs,
        }
    }

    pub fn supports_inspector_tab(&self, tab: InspectorTab) -> bool {
        self.supported_inspector_tabs.contains(&tab)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_supports_all_inspector_tabs() {
        let caps = DbCapabilities::postgres();

        assert!(caps.supports_explain);
        assert!(caps.supports_pg_service_entries);
        assert!(caps.supports_inspector_tab(InspectorTab::Ddl));
        assert_eq!(caps.supported_inspector_tabs().len(), 7);
    }

    #[test]
    fn normalize_unsupported_tab_returns_first_supported_tab() {
        let caps = DbCapabilities {
            supports_explain: false,
            supports_pg_service_entries: false,
            supported_inspector_tabs: vec![InspectorTab::Info, InspectorTab::Columns],
        };

        assert_eq!(
            caps.normalize_inspector_tab(InspectorTab::Triggers),
            InspectorTab::Info
        );
    }
}
