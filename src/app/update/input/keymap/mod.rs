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
use crate::ports::inbound::KeyCombo;
use crate::update::action::Action;

pub use browse::{BrowseShortcut, browse_hints, browse_shortcut};
pub use cell_detail::cell_detail_hints;
pub use cell_edit::cell_edit_hints;
pub use command::{LineCommand, command_line_hints, parse_command};
pub use confirm::confirm_hints;
pub use connection_error::connection_error_hints;
pub use connection_selector::connection_selector_hints;
pub use connection_setup::connection_setup_hints;
pub use context::KeyContext;
pub use global::{PaletteEntry, palette_action, palette_available, palette_entry};
pub use help::help_hints;
pub use json_edit::json_edit_hints;
pub use json_search::json_search_hints;
pub use json_view::json_view_hints;
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
    reference_scopes(context)
        .iter()
        .filter_map(|scope| scope_rows(scope).map(|rows| model::hints(rows, scope, surface)))
        .flatten()
        .collect()
}

fn reference_scopes(context: &KeyContext) -> Vec<KeyContext> {
    use context::BrowsePane;
    use context::SqlConfirmation;

    let mut scopes = Vec::new();
    for pane in [
        BrowsePane::Explorer,
        BrowsePane::Inspector,
        BrowsePane::ResultScroll,
        BrowsePane::ResultCell,
    ] {
        for staged in [false, true] {
            scopes.push(context.clone().with_browse_reference(pane, staged, false));
        }
        if pane == BrowsePane::ResultCell {
            scopes.push(context.clone().with_browse_reference(pane, false, true));
        }
    }
    scopes.push(
        context
            .clone()
            .with_browse_reference(BrowsePane::Explorer, false, false)
            .with_browse_reference_error(),
    );
    scopes.push(
        context
            .clone()
            .with_browse_reference(BrowsePane::ResultScroll, false, false)
            .with_browse_reference_focus_read_only(),
    );
    let make = |mode| KeyContext::for_scope(mode, context.preset(), context.policy());
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
    scopes
}

pub fn current_hints(context: &KeyContext) -> Vec<Hint> {
    scope_hints(HintSurface::Current, context)
}

pub fn footer_hints(context: &KeyContext) -> Vec<Hint> {
    scope_hints(HintSurface::Footer, context)
}

fn scope_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    if context.mode() == InputMode::Normal {
        browse_hints(surface, context)
    } else {
        scope_rows(context).map_or_else(Vec::new, |rows| model::hints(rows, context, surface))
    }
}

pub(crate) fn key_action(combo: KeyCombo, context: &KeyContext) -> Action {
    scope_rows(context)
        .and_then(|rows| model::resolve(rows, combo, context))
        .unwrap_or(Action::None)
}
