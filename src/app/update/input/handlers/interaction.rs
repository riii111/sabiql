use crate::model::app_state::AppState;
use crate::model::browse::jsonb_detail::JsonbDetailMode;
use crate::model::shared::help::HelpMode;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::update::action::InputTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputInteraction {
    Viewing,
    Editing(InputTarget),
}

pub fn resolve_input_interaction(state: &AppState) -> InputInteraction {
    match state.input_mode() {
        InputMode::CommandLine => InputInteraction::Editing(InputTarget::CommandLine),
        InputMode::CellEdit => InputInteraction::Editing(InputTarget::ResultCellEdit),
        InputMode::TablePicker => InputInteraction::Editing(InputTarget::Filter),
        InputMode::ErTablePicker => InputInteraction::Editing(InputTarget::ErFilter),
        InputMode::QueryHistoryPicker => InputInteraction::Editing(InputTarget::QueryHistoryFilter),
        InputMode::Settings if state.settings.is_editing_custom_er_browser() => {
            InputInteraction::Editing(InputTarget::SettingsErBrowser)
        }
        InputMode::ConnectionSetup if state.connection_setup.focused_input().is_some() => {
            InputInteraction::Editing(InputTarget::ConnectionSetup)
        }
        InputMode::SqlModal => match state.sql_modal.status() {
            SqlModalStatus::Editing => InputInteraction::Editing(InputTarget::SqlModal),
            SqlModalStatus::ConfirmingHigh { .. } => {
                InputInteraction::Editing(InputTarget::SqlModalHighRisk)
            }
            SqlModalStatus::ConfirmingAnalyzeHigh { .. } => {
                InputInteraction::Editing(InputTarget::SqlModalAnalyzeHighRisk)
            }
            _ => InputInteraction::Viewing,
        },
        InputMode::JsonbDetail => match state.jsonb_detail.mode() {
            JsonbDetailMode::Viewing => InputInteraction::Viewing,
            JsonbDetailMode::Editing => InputInteraction::Editing(InputTarget::JsonbEdit),
            JsonbDetailMode::Searching => InputInteraction::Editing(InputTarget::JsonbSearch),
        },
        InputMode::JsonbEdit => InputInteraction::Editing(InputTarget::JsonbEdit),
        InputMode::Help => match state.ui.help.mode() {
            HelpMode::Viewing => InputInteraction::Viewing,
            HelpMode::EditingFilter => InputInteraction::Editing(InputTarget::HelpFilter),
        },
        _ => InputInteraction::Viewing,
    }
}
