use crate::app::action::Action;
use crate::app::input_mode::InputMode;

#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub title: String,
    pub message: String,
    pub on_confirm: Action,
    pub on_cancel: Action,
    /// The InputMode to return to after dialog closes
    pub return_mode: InputMode,
}

impl Default for ConfirmDialogState {
    fn default() -> Self {
        Self {
            title: "Confirm".to_string(),
            message: String::new(),
            on_confirm: Action::None,
            on_cancel: Action::None,
            return_mode: InputMode::Normal,
        }
    }
}
