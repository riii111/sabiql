use crate::app::action::Action;

#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub title: String,
    pub message: String,
    pub on_confirm: Action,
    pub on_cancel: Action,
}

impl Default for ConfirmDialogState {
    fn default() -> Self {
        Self {
            title: "Confirm".to_string(),
            message: String::new(),
            on_confirm: Action::None,
            on_cancel: Action::None,
        }
    }
}
