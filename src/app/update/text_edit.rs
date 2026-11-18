use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::model::app_state::AppState;
use crate::model::shared::text_input::TextEdit;
use crate::update::action::{DetailTarget, InputTarget};
use crate::update::browse::navigation::input::edit_command_line;
use crate::update::browse::result::detail_search::edit_search_query;
use crate::update::browse::result::json::edit_json_text;
use crate::update::connection::setup::edit_text as edit_connection_setup;
use crate::update::sql_editor::editing::edit_text as edit_sql_modal;
use crate::update::sql_editor::high_risk::edit_text as edit_high_risk_confirmation;

pub(in crate::update) fn apply_text_edit(
    state: &mut AppState,
    target: InputTarget,
    edit: TextEdit,
    now: Instant,
) -> Vec<Effect> {
    let yank_text = state.kill_buffer().map(str::to_owned);
    // Every target ignores a yank from an empty kill buffer, including side effects such as
    // selection resets, so the router stops it before reaching the target.
    if edit == TextEdit::Yank && yank_text.is_none() {
        return vec![];
    }
    let yank_text = yank_text.as_deref();

    let killed = match target {
        InputTarget::SqlModal => edit_sql_modal(state, edit, yank_text, now),
        InputTarget::SqlModalHighRisk | InputTarget::SqlModalAnalyzeHighRisk => {
            edit_high_risk_confirmation(state, target, edit, yank_text)
        }
        InputTarget::ResultCellEdit => state.result_interaction.edit_cell_text(edit, yank_text),
        InputTarget::ConnectionSetup => edit_connection_setup(state, edit, yank_text),
        InputTarget::CommandLine => edit_command_line(state, edit, yank_text),
        InputTarget::Filter => state
            .ui
            .table_picker_mut()
            .apply_filter_edit(edit, yank_text),
        InputTarget::ErFilter => state.ui.er_picker_mut().apply_filter_edit(edit, yank_text),
        InputTarget::SettingsErBrowser => state
            .settings
            .edit_custom_browser(|input| input.apply_edit(edit, yank_text))
            .unwrap_or_default(),
        InputTarget::QueryHistoryFilter => state
            .query_history_picker
            .apply_filter_edit(edit, yank_text),
        InputTarget::JsonEdit => edit_json_text(state, edit, yank_text),
        InputTarget::JsonSearch => {
            edit_search_query(state, DetailTarget::JsonDetail, edit, yank_text)
        }
        InputTarget::CellDetailSearch => match edit {
            TextEdit::Kill(_) | TextEdit::Yank => {
                debug_assert!(false, "cell detail search does not support {edit:?}");
                String::new()
            }
            _ => edit_search_query(state, DetailTarget::CellDetail, edit, yank_text),
        },
        InputTarget::HelpFilter => state.ui.help_mut().apply_filter_edit(edit, yank_text),
    };
    state.record_kill(killed);
    vec![]
}
