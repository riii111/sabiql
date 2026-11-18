use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::services::AppServices;
use crate::update::action::{
    Action, CursorPosition, ListMotion, ListTarget, ModalKind, ScrollAmount, ScrollDirection,
    ScrollTarget, ScrollToCursorTarget, SelectMotion,
};
use crate::update::reducer::legacy_dispatch;
use crate::update::{connection, explain, modal};

pub(in crate::update) fn scroll(
    state: &mut AppState,
    target: ScrollTarget,
    direction: ScrollDirection,
    amount: ScrollAmount,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    match target {
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
        ScrollTarget::SqliteDiagnostics => {
            modal::sqlite_diagnostics::scroll_sqlite_diagnostics(state, direction, amount)
        }
        _ => legacy_dispatch(
            state,
            Action::Scroll {
                target,
                direction,
                amount,
            },
            now,
            services,
        ),
    }
}

pub(in crate::update) fn scroll_to_cursor(
    state: &mut AppState,
    target: ScrollToCursorTarget,
    position: CursorPosition,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    legacy_dispatch(
        state,
        Action::ScrollToCursor { target, position },
        now,
        services,
    )
}

pub(in crate::update) fn select(
    state: &mut AppState,
    motion: SelectMotion,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    legacy_dispatch(state, Action::Select(motion), now, services)
}

pub(in crate::update) fn list_select(
    state: &mut AppState,
    target: ListTarget,
    motion: ListMotion,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    match target {
        ListTarget::QueryHistory => modal::query_history::list_select_query_history(state, motion),
        _ => legacy_dispatch(state, Action::ListSelect { target, motion }, now, services),
    }
}

pub(in crate::update) fn open_modal(
    state: &mut AppState,
    kind: ModalKind,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    match kind {
        ModalKind::TablePicker => modal::base::open_modal_table_picker(state),
        ModalKind::CommandPalette => modal::base::open_modal_command_palette(state),
        ModalKind::Settings => modal::settings::open_modal_settings(state),
        ModalKind::ErTablePicker => modal::er_picker::open_modal_er_table_picker(state, now),
        ModalKind::QueryHistoryPicker => {
            modal::query_history::open_modal_query_history_picker(state)
        }
        ModalKind::ConnectionSetup => connection::setup::open_modal_connection_setup(state),
        ModalKind::ConnectionSelector => {
            connection::selector::open_modal_connection_selector(state)
        }
        ModalKind::SqliteDiagnostics => {
            modal::sqlite_diagnostics::open_modal_sqlite_diagnostics(state)
        }
        _ => legacy_dispatch(state, Action::OpenModal(kind), now, services),
    }
}

pub(in crate::update) fn close_modal(
    state: &mut AppState,
    kind: ModalKind,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
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
        ModalKind::ConnectionSetup => connection::setup::close_modal_connection_setup(state),
        ModalKind::SqliteDiagnostics => {
            modal::sqlite_diagnostics::close_modal_sqlite_diagnostics(state)
        }
        _ => legacy_dispatch(state, Action::CloseModal(kind), now, services),
    }
}

pub(in crate::update) fn paste(
    state: &mut AppState,
    text: String,
    now: Instant,
    services: &AppServices,
) -> Vec<Effect> {
    match state.input_mode() {
        InputMode::ConnectionSetup => connection::setup::paste_connection_setup(state, text),
        _ => legacy_dispatch(state, Action::Paste(text), now, services),
    }
}
