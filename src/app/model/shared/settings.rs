use super::theme_id::ThemeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsState {
    previous_theme: ThemeId,
    selected_theme: ThemeId,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            previous_theme: ThemeId::Default,
            selected_theme: ThemeId::Default,
        }
    }
}

impl SettingsState {
    pub fn open(&mut self, current_theme: ThemeId) {
        self.previous_theme = current_theme;
        self.selected_theme = current_theme;
    }

    pub fn previous_theme(&self) -> ThemeId {
        self.previous_theme
    }

    pub fn selected_theme(&self) -> ThemeId {
        self.selected_theme
    }

    pub fn select_next_theme(&mut self) -> ThemeId {
        self.selected_theme = self.selected_theme.next();
        self.selected_theme
    }

    pub fn select_previous_theme(&mut self) -> ThemeId {
        self.selected_theme = self.selected_theme.previous();
        self.selected_theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_tracks_previous_and_selected_theme() {
        let mut state = SettingsState::default();

        state.open(ThemeId::Light);

        assert_eq!(state.previous_theme(), ThemeId::Light);
        assert_eq!(state.selected_theme(), ThemeId::Light);
    }

    #[test]
    fn selection_moves_between_themes() {
        let mut state = SettingsState::default();
        state.open(ThemeId::Default);

        assert_eq!(state.select_next_theme(), ThemeId::Light);
        assert_eq!(state.select_previous_theme(), ThemeId::Default);
    }
}
