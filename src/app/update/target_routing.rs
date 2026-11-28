use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::picker::sanitize_filter_text;
use crate::policy::FeaturePolicy;
use crate::services::AppServices;
use crate::update::action::{
    CursorPosition, DetailTarget, InputTarget, ListMotion, ListTarget, ModalKind, ScrollAmount,
    ScrollDirection, ScrollTarget, ScrollToCursorTarget,
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
) {
    use browse::{
        navigation::{explorer::scroll_explorer, inspector::scroll_inspector},
        result::{
            cell_detail::scroll_cell_detail, row_detail::scroll_row_detail, scroll::scroll_result,
        },
    };
    use connection::error::scroll_connection_error;
    use explain::scroll::{scroll_explain_compare, scroll_explain_confirm, scroll_explain_plan};
    use modal::{
        confirm_dialog::scroll_confirm_dialog, help::scroll_help,
        sqlite_diagnostics::scroll_sqlite_diagnostics,
    };

    match target {
        ScrollTarget::Result => scroll_result(state, direction, amount),
        ScrollTarget::Inspector => scroll_inspector(state, direction, amount, services),
        ScrollTarget::Help => scroll_help(state, direction, amount),
        ScrollTarget::ConnectionError => scroll_connection_error(state, direction, amount),
        ScrollTarget::ConfirmDialog => scroll_confirm_dialog(state, direction, amount),
        ScrollTarget::ExplainPlan => scroll_explain_plan(state, direction, amount),
        ScrollTarget::ExplainCompare => scroll_explain_compare(state, direction, amount),
        ScrollTarget::ExplainConfirm => scroll_explain_confirm(state, direction, amount),
        ScrollTarget::Explorer => scroll_explorer(state, direction, amount),
        ScrollTarget::CellDetail => scroll_cell_detail(state, direction, amount),
        ScrollTarget::SqliteDiagnostics => scroll_sqlite_diagnostics(state, direction, amount),
        ScrollTarget::RowDetail => scroll_row_detail(state, direction, amount),
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
    // Reducers also open modals directly, so the engine's feature gate is checked here rather than only for the action.
    if !FeaturePolicy::new(&state.session.active_engine_feature_profile())
        .is_enabled(kind.feature_requirement())
    {
        return vec![];
    }
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
        ModalKind::Help => {
            modal::help::close_help(state);
            vec![]
        }
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
        InputTarget::Filter => {
            state.ui.table_picker_mut().insert_filter_str(&text);
            vec![]
        }
        InputTarget::ErFilter => {
            state.ui.er_picker_mut().insert_filter_str(&text);
            vec![]
        }
        InputTarget::SettingsErBrowser => {
            state
                .settings
                .edit_custom_browser(|input| input.insert_str(&sanitize_filter_text(&text)));
            vec![]
        }
        InputTarget::QueryHistoryFilter => {
            state.query_history_picker.insert_filter_str(&text);
            vec![]
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
        InputTarget::HelpFilter => {
            state
                .ui
                .help_mut()
                .edit_filter(|input| input.insert_str(&sanitize_filter_text(&text)));
            vec![]
        }
    }
}
