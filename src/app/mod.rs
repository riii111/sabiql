pub mod cmd;
pub mod model;
pub mod policy;
pub mod update;

// ── Unchanged top-level modules ─────────────────────────────────
pub mod ports;
pub mod services;

// ── Not yet moved (Phase 4) ──────────────────────────────────────
pub mod cache;
pub mod completion;
pub mod effect;
pub(crate) mod effect_handlers;
pub mod effect_runner;
pub mod er_task;
pub mod render_schedule;

// ── Backward-compat re-exports for update/ (Phase 3) ────────────
pub use update::action;
pub use update::input::command;
pub use update::input::keybindings;
pub use update::input::keymap;
pub use update::input::nav_intent;
pub use update::input::palette;
pub use update::reducer;
// reducers re-export as module for backward compat
pub mod reducers {
    pub use super::update::helpers::{
        char_count, char_to_byte_index, insert_char_at_cursor, insert_str_at_cursor, validate_all,
        validate_field,
    };
    pub use super::update::reduce_connection;
    pub use super::update::reduce_er;
    pub use super::update::reduce_explain;
    pub use super::update::reduce_metadata;
    pub use super::update::reduce_modal;
    pub use super::update::reduce_navigation;
    pub use super::update::reduce_query;
    pub use super::update::reduce_result;
    pub use super::update::reduce_sql_modal;
}
// ── Backward-compat re-exports for policy/ (Phase 2) ────────────
pub use policy::sql::lexer as sql_lexer;
pub use policy::sql::statement_classifier;
pub use policy::write::sql_risk;
pub use policy::write::write_guardrails;
pub use policy::write::write_update;

// ── Backward-compat re-exports for model/ (Phase 1) ────────────
pub use model::app_state as state;
pub use model::browse::cell_edit as cell_edit_state;
pub use model::browse::query_execution;
pub use model::browse::result_history;
pub use model::browse::result_interaction;
pub use model::browse::session as browse_session;
pub use model::connection::cache as connection_cache;
pub use model::connection::error as connection_error;
pub use model::connection::error_state as connection_error_state;
pub use model::connection::list as connection_list;
pub use model::connection::setup as connection_setup_state;
pub use model::connection::state as connection_state;
pub use model::er_state;
pub use model::explain_context;
pub use model::runtime_state;
pub use model::shared::confirm_dialog as confirm_dialog_state;
pub use model::shared::flash_timer;
pub use model::shared::focused_pane;
pub use model::shared::input_mode;
pub use model::shared::inspector_tab;
pub use model::shared::key_sequence;
pub use model::shared::message as message_state;
pub use model::shared::modal as modal_state;
pub use model::shared::picker as picker_state;
pub use model::shared::text_input;
pub use model::shared::ui_state;
pub use model::shared::viewport;
pub use model::sql_editor::query_history as query_history_state;

// sql_modal_context re-export: re-exports both modal and completion types
// so that `crate::app::sql_modal_context::*` continues to work.
pub mod sql_modal_context {
    pub use super::model::sql_editor::completion::*;
    pub use super::model::sql_editor::modal::*;
}
