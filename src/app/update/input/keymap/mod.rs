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
use crate::model::shared::input_mode::InputMode;
use crate::ports::inbound::KeyCombo;
use crate::update::action::{Action, InputTarget};

pub(crate) use browse::reference_hints as browse_reference_hints;
pub use browse::{browse_action, browse_hints};
pub use cell_detail::{cell_detail_action, cell_detail_hints};
pub use cell_edit::{cell_edit_action, cell_edit_hints};
pub use command::{command_line_action, command_line_hints, parse_command};
pub(crate) use common::readline_hints as readline_reference_hints;
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

pub(super) fn readline_action_for(combo: &KeyCombo, target: InputTarget) -> Option<Action> {
    common::readline_action_for(combo, target)
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
