use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::sql_modal_visible_rows;
use crate::update::action::CursorMove;
use crate::update::helpers::reject_pending_mysql_connection_probe;

pub(in crate::update) fn open_modal_sql_modal(state: &mut AppState) -> Vec<Effect> {
    if reject_pending_mysql_connection_probe(state) {
        return vec![];
    }
    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.open_sql_tab();
    state.flash_timers.clear(FlashId::SqlModal);
    vec![]
}

pub(in crate::update) fn sql_modal_append_insert(state: &mut AppState) -> Vec<Effect> {
    state.sql_modal.editor.move_cursor(CursorMove::LineEnd);
    state
        .sql_modal
        .editor
        .update_scroll(sql_modal_visible_rows(state.ui.terminal_height));
    state.sql_modal.enter_editing();
    vec![]
}
