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

use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::connection::setup::ConnectionField;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::SqlModalTab;
use crate::policy::FeatureRequirement;

pub use browse::{BrowseShortcut, browse_action, browse_hints, browse_shortcut};
pub use cell_detail::{cell_detail_action, cell_detail_hints};
pub use cell_edit::{cell_edit_action, cell_edit_hints};
pub use command::{LineCommand, command_line_action, command_line_hints, parse_command};
pub use confirm::{confirm_action, confirm_hints};
pub use connection_error::{connection_error_action, connection_error_hints};
pub use connection_selector::{connection_selector_action, connection_selector_hints};
pub use connection_setup::{connection_setup_action, connection_setup_hints};
pub use context::KeyContext;
pub use global::{PaletteEntry, palette_action, palette_available, palette_entry};
pub use help::{help_action, help_hints};
pub use json_edit::{json_edit_action, json_edit_hints};
pub use json_search::{json_search_action, json_search_hints};
pub use json_view::{json_view_action, json_view_hints};
pub use pickers::{Picker, picker_action, picker_hints};
pub use row_detail::{row_detail_action, row_detail_hints};
pub use settings::{settings_action, settings_hints};
pub use sql_confirmation::{sql_confirmation_action, sql_confirmation_hints};
pub use sql_edit::{sql_edit_action, sql_edit_hints};
pub use sql_view::{sql_view_action, sql_view_hints};
pub use sqlite_diagnostics::{sqlite_diagnostics_action, sqlite_diagnostics_hints};

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
    Write,
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
        InputMode::JsonDetail if context.json_mode() == JsonDetailMode::Searching => {
            &json_search::ROWS
        }
        InputMode::JsonDetail => &json_view::ROWS,
        InputMode::JsonEdit => &json_edit::ROWS,
        InputMode::CellDetail if context.is_cell_detail_searching() => &cell_detail::SEARCHING,
        InputMode::CellDetail => &cell_detail::VIEWING,
        InputMode::RowDetail => &row_detail::ROWS,
    })
}

pub(crate) fn reference_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    use context::SqlConfirmation;

    let mut result = browse::reference_hints(surface, context);
    let make = |mode| KeyContext::for_scope(mode, context.preset(), context.policy());
    let mut scopes = Vec::new();
    for mode in [
        InputMode::CommandLine,
        InputMode::CellEdit,
        InputMode::TablePicker,
        InputMode::CommandPalette,
        InputMode::Settings,
        InputMode::Help,
        InputMode::ConnectionError,
        InputMode::ConfirmDialog,
        InputMode::ConnectionSelector,
        InputMode::QueryHistoryPicker,
        InputMode::CellDetail,
        InputMode::RowDetail,
    ] {
        scopes.push(make(mode));
    }
    scopes.push(make(InputMode::Settings).with_settings_editing());
    scopes.push(make(InputMode::Help).with_help_editing());
    scopes.push(make(InputMode::CellDetail).with_cell_detail_searching(true));
    for (field, open) in [
        (ConnectionField::DatabaseType, false),
        (ConnectionField::DatabaseType, true),
        (ConnectionField::Name, false),
    ] {
        scopes.push(make(InputMode::ConnectionSetup).with_connection_setup(field, open));
    }
    let sql = make(InputMode::SqlModal);
    scopes.push(sql.clone().with_sql_view(SqlModalTab::Sql));
    scopes.push(sql.clone().with_sql_editing(false));
    scopes.push(sql.clone().with_sql_editing(true));
    for kind in [SqlConfirmation::High, SqlConfirmation::Risk] {
        scopes.push(sql.clone().with_sql_confirmation(kind));
    }
    if context
        .policy()
        .is_enabled(FeatureRequirement::ExplainAnalyze)
    {
        for kind in [SqlConfirmation::AnalyzeHigh, SqlConfirmation::AnalyzeRisk] {
            scopes.push(sql.clone().with_sql_confirmation(kind));
        }
    }
    if context.policy().is_enabled(FeatureRequirement::Explain) {
        scopes.push(sql.clone().with_sql_view(SqlModalTab::Plan));
    }
    if context
        .policy()
        .is_enabled(FeatureRequirement::PlanComparison)
    {
        scopes.push(sql.with_sql_view(SqlModalTab::Compare));
    }
    if context.policy().is_enabled(FeatureRequirement::ErDiagram) {
        scopes.push(make(InputMode::ErTablePicker));
    }
    if context
        .policy()
        .is_enabled(FeatureRequirement::SqliteDiagnostics)
    {
        scopes.push(make(InputMode::SqliteDiagnostics));
    }
    if context
        .policy()
        .is_enabled(FeatureRequirement::JsonDocumentDetail)
    {
        scopes.push(make(InputMode::JsonDetail).with_json_mode(JsonDetailMode::Viewing));
        scopes.push(make(InputMode::JsonDetail).with_json_mode(JsonDetailMode::Searching));
    }
    if context
        .policy()
        .is_enabled(FeatureRequirement::JsonDocumentEdit)
    {
        scopes.push(make(InputMode::JsonEdit).with_json_edit());
    }
    for scope in scopes {
        if let Some(rows) = scope_rows(&scope) {
            result.extend(model::hints(rows, &scope, surface));
        }
    }
    result.extend(command::reference_hints(surface, context));
    result
}

pub fn current_hints(context: &KeyContext) -> Vec<Hint> {
    let surface = HintSurface::Current;
    match context.mode() {
        InputMode::Normal => browse_hints(surface, context),
        InputMode::CommandLine => command_line_hints(surface, context),
        InputMode::CellEdit => cell_edit_hints(surface, context),
        InputMode::TablePicker => picker_hints(Picker::Table, surface, context),
        InputMode::CommandPalette => picker_hints(Picker::CommandPalette, surface, context),
        InputMode::Settings => settings_hints(surface, context),
        InputMode::Help => help_hints(surface, context),
        InputMode::SqlModal if context.is_sql_editing() => sql_edit_hints(surface, context),
        InputMode::SqlModal if context.sql_confirmation().is_some() => {
            sql_confirmation_hints(surface, context)
        }
        InputMode::SqlModal => sql_view_hints(surface, context),
        InputMode::ConnectionSetup => connection_setup_hints(surface, context),
        InputMode::ConnectionError => connection_error_hints(surface, context),
        InputMode::SqliteDiagnostics => sqlite_diagnostics_hints(surface, context),
        InputMode::ConfirmDialog => confirm_hints(surface, context),
        InputMode::ConnectionSelector => connection_selector_hints(surface, context),
        InputMode::ErTablePicker => picker_hints(Picker::Er, surface, context),
        InputMode::QueryHistoryPicker => picker_hints(Picker::QueryHistory, surface, context),
        InputMode::JsonDetail => match context.json_mode() {
            JsonDetailMode::Viewing => json_view_hints(surface, context),
            JsonDetailMode::Searching => json_search_hints(surface, context),
            JsonDetailMode::Editing => json_edit_hints(surface, context),
        },
        InputMode::JsonEdit => json_edit_hints(surface, context),
        InputMode::CellDetail => cell_detail_hints(surface, context),
        InputMode::RowDetail => row_detail_hints(surface, context),
    }
}
