mod browse;
mod cell_detail;
mod cell_edit;
mod command;
mod common;
mod confirm;
mod connection_error;
mod connection_selector;
mod connection_setup;
mod context;
mod global;
mod help;
mod json_edit;
mod json_search;
mod json_view;
mod model;
mod pickers;
mod row_detail;
mod scroll;
mod settings;
mod sql_confirmation;
mod sql_edit;
mod sql_view;
mod sqlite_diagnostics;

use crate::model::shared::input_mode::InputMode;
use crate::policy::FeatureRequirement;
use crate::ports::inbound::KeyCombo;
use crate::update::action::{Action, InputTarget};

pub use browse::{BrowseShortcut, browse_hints, browse_shortcut};
pub use cell_detail::cell_detail_hints;
pub use command::{LineCommand, parse_command};
pub use confirm::confirm_hints;
pub use connection_error::connection_error_hints;
pub use connection_selector::connection_selector_hints;
pub use connection_setup::connection_setup_hints;
pub use context::KeyContext;
pub use global::{
    PaletteEntry, listed_palette_entries, palette_action, palette_available, palette_entry,
};
pub use help::help_hints;
pub use pickers::{Picker, picker_hints};
pub use row_detail::row_detail_hints;
pub use settings::settings_hints;
pub use sql_confirmation::sql_confirmation_hints;
pub use sql_edit::sql_edit_hints;
pub use sql_view::sql_view_hints;
pub use sqlite_diagnostics::sqlite_diagnostics_hints;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintSurface {
    Footer,
    Border,
    Current,
    SearchFilter,
    Advanced,
    ModalBasics,
    Connections,
    Details,
    Editing,
    DataActions,
    ExplainPlaceholder,
    Navigation,
    Common,
    OpenSwitch,
    Palette,
    FooterAction,
    FooterNavigation,
    FooterHelp,
    FooterClose,
    FooterQuit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub key: String,
    pub short: &'static str,
    pub description: &'static str,
}

fn scope_rows(context: &KeyContext) -> Option<&'static [model::Row]> {
    Some(match context.mode() {
        InputMode::Normal => browse::active_rows(context),
        InputMode::CommandLine => &command::EDITING,
        InputMode::CellEdit => &cell_edit::ROWS,
        InputMode::TablePicker => &pickers::TABLE_ROWS,
        InputMode::CommandPalette => &pickers::PALETTE_ROWS,
        InputMode::Settings if context.is_settings_editing() => &settings::EDITING,
        InputMode::Settings => &settings::SELECTING,
        InputMode::Help if context.is_help_editing() => &help::EDITING,
        InputMode::Help => &help::VIEWING,
        InputMode::SqlModal if context.is_sql_editing() => &sql_edit::ROWS,
        InputMode::SqlModal => {
            sql_confirmation::active_rows(context).or_else(|| sql_view::active_rows(context))?
        }
        InputMode::ConnectionSetup => connection_setup::active_rows(context),
        InputMode::ConnectionError => &connection_error::ROWS,
        InputMode::ConfirmDialog => &confirm::ROWS,
        InputMode::SqliteDiagnostics => &sqlite_diagnostics::ROWS,
        InputMode::ConnectionSelector => &connection_selector::ROWS,
        InputMode::ErTablePicker => &pickers::ER_ROWS,
        InputMode::QueryHistoryPicker => &pickers::HISTORY_ROWS,
        InputMode::JsonDetail if context.json_searching => &json_search::ROWS,
        InputMode::JsonDetail => &json_view::ROWS,
        InputMode::JsonEdit => &json_edit::ROWS,
        InputMode::CellDetail if context.is_cell_detail_searching() => &cell_detail::SEARCHING,
        InputMode::CellDetail => &cell_detail::VIEWING,
        InputMode::RowDetail => &row_detail::ROWS,
    })
}

pub(crate) fn reference_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    use FeatureRequirement as Feature;

    let groups: &[&[model::Row]] = &[
        &browse::EXPLORER,
        &browse::INSPECTOR,
        &browse::RESULT_SCROLL,
        &browse::RESULT_CELL,
        &command::EDITING,
        &cell_edit::ROWS,
        &pickers::TABLE_ROWS,
        &pickers::PALETTE_ROWS,
        &settings::SELECTING,
        &help::VIEWING,
        &connection_error::ROWS,
        &confirm::ROWS,
        &connection_selector::ROWS,
        &pickers::HISTORY_ROWS,
        &cell_detail::VIEWING,
        &row_detail::ROWS,
        &settings::EDITING,
        &help::EDITING,
        &cell_detail::SEARCHING,
        &connection_setup::CONNECT,
        &sql_view::SQL,
        &sql_edit::ROWS,
        &sql_confirmation::HIGH,
        &sql_confirmation::RISK,
    ];
    let feature_groups: &[(Feature, &[model::Row])] = &[
        (Feature::ExplainAnalyze, &sql_confirmation::ANALYZE_HIGH),
        (Feature::ExplainAnalyze, &sql_confirmation::ANALYZE_RISK),
        (Feature::Explain, &sql_view::PLAN),
        (Feature::PlanComparison, &sql_view::COMPARE),
        (Feature::ErDiagram, &pickers::ER_ROWS),
        (Feature::SqliteDiagnostics, &sqlite_diagnostics::ROWS),
        (Feature::JsonDocumentDetail, &json_view::ROWS),
        (Feature::JsonDocumentDetail, &json_search::ROWS),
        (Feature::JsonDocumentEdit, &json_edit::ROWS),
    ];

    let mut result = Vec::new();
    for rows in groups.iter().copied().chain(
        feature_groups
            .iter()
            .filter_map(|(feature, rows)| context.policy().is_enabled(*feature).then_some(*rows)),
    ) {
        let mut deferred = Vec::new();
        for row in rows {
            if !row.surfaces.contains(&surface) && !row.references.contains(&surface) {
                continue;
            }
            let Some(mut hint) = model::reference_hint(row, context.preset(), context.policy())
            else {
                continue;
            };
            if surface == HintSurface::Common && row.dynamic_label.is_some() {
                result.push(hint.clone());
                if hint.description == "Enable Read-Only mode" {
                    let mut focus = result
                        .iter()
                        .find(|h: &&Hint| h.description == "Toggle Focus mode")
                        .unwrap()
                        .clone();
                    focus.description = "Exit Focus mode";
                    result.push(focus);
                    hint.description = "Disable Read-Only mode";
                    result.push(hint);
                }
                continue;
            }
            if surface == HintSurface::DataActions
                && hint.description == "Cancel"
                && row.dynamic_label.is_some()
            {
                result.push(hint.clone());
                hint.description = "Clear all staged row deletions";
                deferred.push(hint);
                continue;
            }
            if surface == HintSurface::DataActions
                && hint.description == "Unstage the last staged row deletion"
            {
                result.append(&mut deferred);
            }
            if surface == HintSurface::Editing
                && hint.description == "Exit cell selection or discard draft"
            {
                hint.description = "Exit cell selection";
                result.push(hint.clone());
                hint.description = "Discard pending draft";
                deferred.push(hint);
                continue;
            }
            if std::ptr::eq(rows, &sql_edit::ROWS[..])
                && matches!(
                    hint.description,
                    "Select completion" | "Accept completion" | "Dismiss completion"
                )
            {
                deferred.push(hint);
                continue;
            }
            if std::ptr::eq(rows, &connection_setup::CONNECT[..]) {
                match hint.description {
                    "Confirm dropdown selection"
                    | "Dropdown navigation"
                    | "Cancel dropdown"
                    | "Edit connection setting" => {
                        deferred.push(hint);
                        continue;
                    }
                    "Save and connect"
                        if context.preset()
                            == crate::model::shared::settings::KeymapPreset::Ide =>
                    {
                        let mut closed = hint.clone();
                        closed.key = "Ctrl+S".to_string();
                        result.push(closed);
                        continue;
                    }
                    _ => {}
                }
            }
            result.push(hint);
        }
        if surface == HintSurface::Connections
            && std::ptr::eq(rows, &connection_setup::CONNECT[..])
            && context.preset() == crate::model::shared::settings::KeymapPreset::Ide
        {
            let insert_at = deferred.len().saturating_sub(1);
            if let Some(row) = rows
                .iter()
                .find(|row| row.description == "Save and connect")
                && let Some(hint) = model::reference_hint(row, context.preset(), context.policy())
            {
                deferred.insert(insert_at, hint);
            }
        }
        result.extend(deferred);
    }
    result
}

pub fn current_hints(context: &KeyContext) -> Vec<Hint> {
    scope_hints(HintSurface::Current, context)
}

pub fn footer_hints(context: &KeyContext) -> Vec<Hint> {
    scope_hints(HintSurface::Footer, context)
}

pub fn border_hints(context: &KeyContext) -> Vec<Hint> {
    scope_hints(HintSurface::Border, context)
}

fn scope_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    if context.mode() == InputMode::Normal {
        browse_hints(surface, context)
    } else {
        scope_rows(context).map_or_else(Vec::new, |rows| model::hints(rows, context, surface))
    }
}

/// Returns the input that receives typed characters in the current scope, or `None` while viewing.
pub(crate) fn paste_target(context: &KeyContext) -> Option<InputTarget> {
    scope_rows(context).and_then(|rows| model::text_target(rows, context))
}

pub(crate) fn key_action(combo: KeyCombo, context: &KeyContext) -> Action {
    scope_rows(context)
        .and_then(|rows| model::resolve(rows, combo, context))
        .unwrap_or(Action::None)
}
