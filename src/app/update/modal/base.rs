use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::update::input::keymap::{KeyContext, palette_available};

pub(in crate::update) fn open_modal_table_picker(state: &mut AppState) -> Vec<Effect> {
    state.modal.set_mode(InputMode::TablePicker);
    state.ui.table_picker_mut().clear_filter_and_reset();
    vec![]
}

pub(in crate::update) fn close_modal_table_picker_or_command_palette(
    state: &mut AppState,
) -> Vec<Effect> {
    state.modal.set_mode(InputMode::Normal);
    vec![]
}

pub(in crate::update) fn escape(state: &mut AppState) -> Vec<Effect> {
    match state.modal.active_mode() {
        InputMode::Normal | InputMode::ConnectionSelector => {
            state.modal.set_mode(InputMode::Normal);
        }
        _ => {}
    }
    vec![]
}

pub(in crate::update) fn open_modal_command_palette(state: &mut AppState) -> Vec<Effect> {
    let commands = palette_available(&KeyContext::for_browse_state(state))
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    state.ui.set_palette_commands(commands);
    state.modal.set_mode(InputMode::CommandPalette);
    // Command palette currently reuses the generic picker selection and filter state.
    state.ui.table_picker_mut().clear_filter_and_reset();
    vec![]
}

pub(in crate::update) fn close_modal_sql_modal(state: &mut AppState) -> Vec<Effect> {
    state.modal.set_mode(InputMode::Normal);
    state.sql_modal.cleanup_on_close();
    state.flash_timers.clear(FlashId::SqlModal);
    vec![]
}
