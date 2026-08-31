use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailMode;
use crate::model::shared::help::HelpMode;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::update::action::InputTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputInteraction {
    Viewing,
    FormEditing(InputTarget),
    VimEditing(InputTarget),
}

pub fn resolve_input_interaction(state: &AppState) -> InputInteraction {
    // Readline belongs to self-contained form fields. SQL and JSON document editors keep
    // their Vim contexts so Ctrl/Alt bindings never create a hybrid editing model.
    match state.input_mode() {
        InputMode::CommandLine => InputInteraction::FormEditing(InputTarget::CommandLine),
        InputMode::CellEdit => InputInteraction::FormEditing(InputTarget::ResultCellEdit),
        InputMode::TablePicker => InputInteraction::FormEditing(InputTarget::Filter),
        InputMode::ErTablePicker => InputInteraction::FormEditing(InputTarget::ErFilter),
        InputMode::QueryHistoryPicker => {
            InputInteraction::FormEditing(InputTarget::QueryHistoryFilter)
        }
        InputMode::Settings if state.settings.is_editing_custom_er_browser() => {
            InputInteraction::FormEditing(InputTarget::SettingsErBrowser)
        }
        InputMode::ConnectionSetup if state.connection_setup.focused_input().is_some() => {
            InputInteraction::FormEditing(InputTarget::ConnectionSetup)
        }
        InputMode::SqlModal => match state.sql_modal.status() {
            SqlModalStatus::Editing => InputInteraction::VimEditing(InputTarget::SqlModal),
            SqlModalStatus::ConfirmingHigh { .. } => {
                InputInteraction::FormEditing(InputTarget::SqlModalHighRisk)
            }
            SqlModalStatus::ConfirmingAnalyzeHigh { .. } => {
                InputInteraction::FormEditing(InputTarget::SqlModalAnalyzeHighRisk)
            }
            _ => InputInteraction::Viewing,
        },
        InputMode::JsonDetail => match state.json_detail.mode() {
            JsonDetailMode::Viewing => InputInteraction::Viewing,
            JsonDetailMode::Editing => InputInteraction::VimEditing(InputTarget::JsonEdit),
            JsonDetailMode::Searching => InputInteraction::FormEditing(InputTarget::JsonSearch),
        },
        InputMode::JsonEdit => InputInteraction::VimEditing(InputTarget::JsonEdit),
        InputMode::Help => match state.ui.help().mode() {
            HelpMode::Viewing => InputInteraction::Viewing,
            HelpMode::EditingFilter => InputInteraction::FormEditing(InputTarget::HelpFilter),
        },
        _ => InputInteraction::Viewing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_editors_resolve_to_vim_editing() {
        let mut sql = AppState::new("test".to_string());
        sql.modal.set_mode(InputMode::SqlModal);
        sql.sql_modal.enter_editing();

        let mut json_detail = AppState::new("test".to_string());
        json_detail.modal.set_mode(InputMode::JsonDetail);
        json_detail.json_detail.enter_edit();

        let mut json_editor = AppState::new("test".to_string());
        json_editor.modal.set_mode(InputMode::JsonEdit);

        assert_eq!(
            resolve_input_interaction(&sql),
            InputInteraction::VimEditing(InputTarget::SqlModal)
        );
        assert_eq!(
            resolve_input_interaction(&json_detail),
            InputInteraction::VimEditing(InputTarget::JsonEdit)
        );
        assert_eq!(
            resolve_input_interaction(&json_editor),
            InputInteraction::VimEditing(InputTarget::JsonEdit)
        );
    }
}
