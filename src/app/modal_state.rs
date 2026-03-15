use super::input_mode::InputMode;

#[derive(Debug, Clone, Default)]
pub struct ModalState {
    mode: InputMode,
    return_stack: Vec<InputMode>,
}

impl ModalState {
    pub fn active_mode(&self) -> InputMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
        self.return_stack.clear();
    }

    pub fn push_mode(&mut self, mode: InputMode) {
        self.return_stack.push(self.mode);
        self.mode = mode;
    }

    pub fn pop_mode(&mut self) -> InputMode {
        self.mode = self.return_stack.pop().unwrap_or(InputMode::Normal);
        self.mode
    }

    pub fn replace_mode(&mut self, mode: InputMode) {
        self.mode = mode;
    }

    pub fn pop_mode_override(&mut self, target: InputMode) {
        self.return_stack.pop();
        self.mode = target;
    }

    pub fn return_destination(&self) -> InputMode {
        self.return_stack
            .last()
            .copied()
            .unwrap_or(InputMode::Normal)
    }

    pub fn is_modal_active(&self) -> bool {
        !matches!(self.mode, InputMode::Normal | InputMode::CellEdit)
    }

    /// Internal method for bidirectional sync bridge.
    /// Sets mode without clearing the stack.
    pub fn set_mode_raw(&mut self, mode: InputMode) {
        self.mode = mode;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mode_is_normal() {
        let modal = ModalState::default();

        assert_eq!(modal.active_mode(), InputMode::Normal);
        assert!(!modal.is_modal_active());
    }

    #[test]
    fn set_mode_changes_mode_and_clears_stack() {
        let mut modal = ModalState::default();
        modal.push_mode(InputMode::CommandLine);

        modal.set_mode(InputMode::TablePicker);

        assert_eq!(modal.active_mode(), InputMode::TablePicker);
        assert_eq!(modal.return_destination(), InputMode::Normal);
    }

    #[test]
    fn push_pop_preserves_return_mode() {
        let mut modal = ModalState::default();

        modal.push_mode(InputMode::CommandLine);

        assert_eq!(modal.active_mode(), InputMode::CommandLine);
        assert_eq!(modal.return_destination(), InputMode::Normal);

        let returned = modal.pop_mode();

        assert_eq!(returned, InputMode::Normal);
        assert_eq!(modal.active_mode(), InputMode::Normal);
    }

    #[test]
    fn push_from_non_normal_preserves_origin() {
        let mut modal = ModalState::default();
        modal.set_mode(InputMode::SqlModal);

        modal.push_mode(InputMode::QueryHistoryPicker);

        assert_eq!(modal.active_mode(), InputMode::QueryHistoryPicker);
        assert_eq!(modal.return_destination(), InputMode::SqlModal);

        let returned = modal.pop_mode();
        assert_eq!(returned, InputMode::SqlModal);
    }

    #[test]
    fn nested_push_pop() {
        let mut modal = ModalState::default();

        modal.push_mode(InputMode::CommandLine);
        modal.push_mode(InputMode::ConfirmDialog);

        assert_eq!(modal.active_mode(), InputMode::ConfirmDialog);

        modal.pop_mode();
        assert_eq!(modal.active_mode(), InputMode::CommandLine);

        modal.pop_mode();
        assert_eq!(modal.active_mode(), InputMode::Normal);
    }

    #[test]
    fn pop_on_empty_stack_returns_normal() {
        let mut modal = ModalState::default();
        modal.set_mode(InputMode::CommandLine);

        let returned = modal.pop_mode();

        assert_eq!(returned, InputMode::Normal);
    }

    #[test]
    fn replace_mode_keeps_stack() {
        let mut modal = ModalState::default();
        modal.set_mode(InputMode::ConnectionSelector);
        modal.push_mode(InputMode::ConnectionSetup);

        modal.replace_mode(InputMode::ConnectionError);

        assert_eq!(modal.active_mode(), InputMode::ConnectionError);
        assert_eq!(modal.return_destination(), InputMode::ConnectionSelector);
    }

    #[test]
    fn pop_mode_override_ignores_stack() {
        let mut modal = ModalState::default();
        modal.push_mode(InputMode::ConfirmDialog);

        modal.pop_mode_override(InputMode::ConnectionSetup);

        assert_eq!(modal.active_mode(), InputMode::ConnectionSetup);
        // Stack entry was consumed
        assert_eq!(modal.return_destination(), InputMode::Normal);
    }

    #[test]
    fn is_modal_active_for_various_modes() {
        let mut modal = ModalState::default();

        modal.set_mode(InputMode::Normal);
        assert!(!modal.is_modal_active());

        modal.set_mode(InputMode::CellEdit);
        assert!(!modal.is_modal_active());

        modal.set_mode(InputMode::CommandLine);
        assert!(modal.is_modal_active());

        modal.set_mode(InputMode::Help);
        assert!(modal.is_modal_active());
    }

    #[test]
    fn set_mode_raw_does_not_clear_stack() {
        let mut modal = ModalState::default();
        modal.push_mode(InputMode::CommandLine);

        modal.set_mode_raw(InputMode::Normal);

        assert_eq!(modal.active_mode(), InputMode::Normal);
        // Stack is preserved
        assert_eq!(modal.return_destination(), InputMode::Normal);
        // But the entry is still there
        let popped = modal.pop_mode();
        assert_eq!(popped, InputMode::Normal);
    }

    #[test]
    fn return_destination_with_empty_stack() {
        let modal = ModalState::default();

        assert_eq!(modal.return_destination(), InputMode::Normal);
    }

    #[test]
    fn return_destination_shows_last_pushed() {
        let mut modal = ModalState::default();
        modal.set_mode(InputMode::SqlModal);
        modal.push_mode(InputMode::QueryHistoryPicker);

        assert_eq!(modal.return_destination(), InputMode::SqlModal);
    }
}
