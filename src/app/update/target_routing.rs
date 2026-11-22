use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::services::AppServices;
use crate::update::action::{
    CursorPosition, DetailTarget, InputTarget, ListMotion, ListTarget, ModalKind, ScrollAmount,
    ScrollDirection, ScrollTarget, ScrollToCursorTarget, SelectMotion,
};
use crate::update::{browse, connection, explain, modal, sql_editor};

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every variant must name its handler so a new variant fails to compile instead of being silently dropped."
)]
pub(in crate::update) fn scroll(
    state: &mut AppState,
    target: ScrollTarget,
    direction: ScrollDirection,
    amount: ScrollAmount,
    services: &AppServices,
) -> Vec<Effect> {
    match target {
        ScrollTarget::Result => browse::result::scroll::scroll_result(state, direction, amount),
        ScrollTarget::Inspector => {
            browse::navigation::inspector::scroll_inspector(state, direction, amount, services)
        }
        ScrollTarget::Help => modal::help::scroll_help(state, direction, amount),
        ScrollTarget::ConnectionError => {
            connection::error::scroll_connection_error(state, direction, amount)
        }
        ScrollTarget::ConfirmDialog => {
            modal::confirm_dialog::scroll_confirm_dialog(state, direction, amount)
        }
        ScrollTarget::ExplainPlan => explain::scroll::scroll_explain_plan(state, direction, amount),
        ScrollTarget::ExplainCompare => {
            explain::scroll::scroll_explain_compare(state, direction, amount)
        }
        ScrollTarget::ExplainConfirm => {
            explain::scroll::scroll_explain_confirm(state, direction, amount)
        }
        ScrollTarget::Explorer => {
            browse::navigation::explorer::scroll_explorer(state, direction, amount)
        }
        ScrollTarget::CellDetail => {
            browse::result::cell_detail::scroll_cell_detail(state, direction, amount)
        }
        ScrollTarget::SqliteDiagnostics => {
            modal::sqlite_diagnostics::scroll_sqlite_diagnostics(state, direction, amount)
        }
        ScrollTarget::RowDetail => {
            browse::result::row_detail::scroll_row_detail(state, direction, amount)
        }
    }
}

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every variant must name its handler so a new variant fails to compile instead of being silently dropped."
)]
pub(in crate::update) fn scroll_to_cursor(
    state: &mut AppState,
    target: ScrollToCursorTarget,
    position: CursorPosition,
) -> Vec<Effect> {
    match target {
        ScrollToCursorTarget::Explorer => {
            browse::navigation::explorer::scroll_to_cursor_explorer(state, position)
        }
        ScrollToCursorTarget::Result => {
            browse::result::scroll::scroll_to_cursor_result(state, position)
        }
    }
}

pub(in crate::update) fn select(state: &mut AppState, motion: SelectMotion) -> Vec<Effect> {
    browse::navigation::explorer::select_explorer(state, motion)
}

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every variant must name its handler so a new variant fails to compile instead of being silently dropped."
)]
pub(in crate::update) fn list_select(
    state: &mut AppState,
    target: ListTarget,
    motion: ListMotion,
) -> Vec<Effect> {
    match target {
        ListTarget::ConnectionList => {
            browse::navigation::connection_list::list_select_connection_list(state, motion)
        }
        ListTarget::QueryHistory => modal::query_history::list_select_query_history(state, motion),
        ListTarget::TablePicker => {
            browse::navigation::input::list_select_table_picker(state, motion)
        }
        ListTarget::ErTablePicker => {
            browse::navigation::input::list_select_er_table_picker(state, motion)
        }
        ListTarget::CommandPalette => {
            browse::navigation::input::list_select_command_palette(state, motion)
        }
    }
}

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every variant must name its handler so a new variant fails to compile instead of being silently dropped."
)]
pub(in crate::update) fn open_modal(
    state: &mut AppState,
    kind: ModalKind,
    now: Instant,
) -> Vec<Effect> {
    match kind {
        ModalKind::TablePicker => modal::base::open_modal_table_picker(state),
        ModalKind::CommandPalette => modal::base::open_modal_command_palette(state),
        ModalKind::Settings => modal::settings::open_modal_settings(state),
        ModalKind::SqlModal => sql_editor::mode::open_modal_sql_modal(state),
        ModalKind::ErTablePicker => modal::er_picker::open_modal_er_table_picker(state, now),
        ModalKind::QueryHistoryPicker => {
            modal::query_history::open_modal_query_history_picker(state)
        }
        ModalKind::JsonDetail => browse::result::json::open_modal_json_detail(state),
        ModalKind::RowDetail => browse::result::row_detail::open_modal_row_detail(state),
        ModalKind::ConnectionSetup => connection::setup::open_modal_connection_setup(state),
        ModalKind::ConnectionSelector => {
            connection::selector::open_modal_connection_selector(state)
        }
        ModalKind::SqliteDiagnostics => {
            modal::sqlite_diagnostics::open_modal_sqlite_diagnostics(state)
        }
        ModalKind::Help | ModalKind::CellDetail => vec![],
    }
}

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every variant must name its handler so a new variant fails to compile instead of being silently dropped."
)]
pub(in crate::update) fn close_modal(state: &mut AppState, kind: ModalKind) -> Vec<Effect> {
    match kind {
        ModalKind::TablePicker | ModalKind::CommandPalette => {
            modal::base::close_modal_table_picker_or_command_palette(state)
        }
        ModalKind::Settings => modal::settings::settings_cancel(state),
        ModalKind::Help => modal::help::close_modal_help(state),
        ModalKind::SqlModal => modal::base::close_modal_sql_modal(state),
        ModalKind::ErTablePicker => modal::er_picker::close_modal_er_table_picker(state),
        ModalKind::QueryHistoryPicker => {
            modal::query_history::close_modal_query_history_picker(state)
        }
        ModalKind::JsonDetail => browse::result::json::close_modal_json_detail(state),
        ModalKind::CellDetail => browse::result::cell_detail::close_modal_cell_detail(state),
        ModalKind::RowDetail => browse::result::row_detail::close_modal_row_detail(state),
        ModalKind::ConnectionSetup => connection::setup::close_modal_connection_setup(state),
        ModalKind::SqliteDiagnostics => {
            modal::sqlite_diagnostics::close_modal_sqlite_diagnostics(state)
        }
        ModalKind::ConnectionSelector => vec![],
    }
}

#[deny(
    clippy::wildcard_enum_match_arm,
    clippy::match_wildcard_for_single_variants,
    reason = "Every variant must name its handler so a new variant fails to compile instead of being silently dropped."
)]
pub(in crate::update) fn paste(
    state: &mut AppState,
    target: InputTarget,
    text: String,
    now: Instant,
) -> Vec<Effect> {
    match target {
        InputTarget::SqlModal => sql_editor::editing::paste_sql_modal(state, text, now),
        InputTarget::SqlModalHighRisk | InputTarget::SqlModalAnalyzeHighRisk => {
            sql_editor::high_risk::paste_text(state, target, text)
        }
        InputTarget::ResultCellEdit => browse::navigation::input::paste_cell_edit(state, text),
        InputTarget::ConnectionSetup => connection::setup::paste_connection_setup(state, text),
        InputTarget::CommandLine => browse::navigation::input::paste_command_line(state, text),
        InputTarget::Filter => browse::navigation::input::paste_table_picker(state, text),
        InputTarget::ErFilter => browse::navigation::input::paste_er_table_picker(state, text),
        InputTarget::SettingsErBrowser => modal::settings::paste_custom_browser(state, text),
        InputTarget::QueryHistoryFilter => {
            browse::navigation::input::paste_query_history_picker(state, text)
        }
        InputTarget::JsonEdit => browse::result::json::paste_json_edit(state, text),
        InputTarget::JsonSearch => browse::result::detail_search::paste_search_query(
            state,
            DetailTarget::JsonDetail,
            &text,
        ),
        InputTarget::CellDetailSearch => browse::result::detail_search::paste_search_query(
            state,
            DetailTarget::CellDetail,
            &text,
        ),
        InputTarget::HelpFilter => modal::help::paste_help_filter(state, text),
    }
}
