mod connection_list;
mod explorer;
mod focus;
mod input;
mod inspector;

use std::time::Instant;

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::inspector_tab::InspectorTab;
use crate::app::services::AppServices;
use crate::app::state::AppState;

fn inspector_total_items(state: &AppState, services: &AppServices) -> usize {
    state
        .session
        .table_detail()
        .map(|t| match state.ui.inspector_tab {
            InspectorTab::Info => 5,
            InspectorTab::Columns => t.columns.len(),
            InspectorTab::Indexes => t.indexes.len(),
            InspectorTab::ForeignKeys => t.foreign_keys.len(),
            InspectorTab::Rls => t.rls.as_ref().map_or(1, |rls| {
                let mut lines = 1;
                if !rls.policies.is_empty() {
                    lines += 2;
                    for policy in &rls.policies {
                        lines += 1;
                        if policy.qual.is_some() {
                            lines += 1;
                        }
                    }
                }
                lines
            }),
            InspectorTab::Triggers => t.triggers.len(),
            InspectorTab::Ddl => services.ddl_generator.ddl_line_count(t),
        })
        .unwrap_or(0)
}

pub(super) fn inspector_max_scroll(state: &AppState, services: &AppServices) -> usize {
    let visible = match state.ui.inspector_tab {
        InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
        _ => state.inspector_visible_rows(),
    };
    inspector_total_items(state, services).saturating_sub(visible)
}

pub(super) fn explorer_item_count(state: &AppState) -> usize {
    state.tables().len()
}

pub fn reduce_navigation(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> Option<Vec<Effect>> {
    focus::reduce(state, action, now)
        .or_else(|| input::reduce(state, action))
        .or_else(|| explorer::reduce(state, action))
        .or_else(|| inspector::reduce(state, action, services))
        .or_else(|| connection_list::reduce(state, action, now))
}
