use super::theme_id::ThemeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Keymap,
}

impl SettingsSection {
    pub const ALL: [Self; 2] = [Self::Appearance, Self::Keymap];

    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Keymap => "Keymap",
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeymapPreset {
    #[default]
    Default,
    Ide,
}

impl KeymapPreset {
    pub const ALL: [Self; 2] = [Self::Default, Self::Ide];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Ide => "IDE",
        }
    }

    pub fn config_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Ide => "ide",
        }
    }

    pub fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" => Some(Self::Default),
            "ide" => Some(Self::Ide),
            _ => None,
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|preset| *preset == self)
            .unwrap_or(0);
        Self::ALL[(index + 1).min(Self::ALL.len() - 1)]
    }

    fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|preset| *preset == self)
            .unwrap_or(0);
        Self::ALL[index.saturating_sub(1)]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
    previous_theme: ThemeId,
    selected_theme: ThemeId,
    saved_keymap_preset: KeymapPreset,
    selected_keymap_preset: KeymapPreset,
    section: SettingsSection,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            previous_theme: ThemeId::Default,
            selected_theme: ThemeId::Default,
            saved_keymap_preset: KeymapPreset::Default,
            selected_keymap_preset: KeymapPreset::Default,
            section: SettingsSection::Appearance,
        }
    }
}

impl SettingsState {
    pub fn load_keymap_preset(&mut self, keymap_preset: KeymapPreset) {
        self.saved_keymap_preset = keymap_preset;
        self.selected_keymap_preset = keymap_preset;
    }

    pub fn open(&mut self, current_theme: ThemeId) {
        self.previous_theme = current_theme;
        self.selected_theme = current_theme;
        self.selected_keymap_preset = self.saved_keymap_preset;
        self.section = SettingsSection::Appearance;
    }

    pub fn previous_theme(&self) -> ThemeId {
        self.previous_theme
    }

    pub fn selected_theme(&self) -> ThemeId {
        self.selected_theme
    }

    pub fn saved_keymap_preset(&self) -> KeymapPreset {
        self.saved_keymap_preset
    }

    pub fn selected_keymap_preset(&self) -> KeymapPreset {
        self.selected_keymap_preset
    }

    pub fn section(&self) -> SettingsSection {
        self.section
    }

    pub fn switch_next_section(&mut self) {
        self.section = self.section.next();
    }

    pub fn switch_previous_section(&mut self) {
        self.section = self.section.previous();
    }

    pub fn select_next(&mut self) {
        match self.section {
            SettingsSection::Appearance => {
                self.selected_theme = self.selected_theme.next();
            }
            SettingsSection::Keymap => {
                self.selected_keymap_preset = self.selected_keymap_preset.next();
            }
        }
    }

    pub fn select_previous(&mut self) {
        match self.section {
            SettingsSection::Appearance => {
                self.selected_theme = self.selected_theme.previous();
            }
            SettingsSection::Keymap => {
                self.selected_keymap_preset = self.selected_keymap_preset.previous();
            }
        }
    }

    pub fn commit_saved(&mut self, theme: ThemeId, keymap_preset: KeymapPreset) {
        self.previous_theme = theme;
        self.selected_theme = theme;
        self.saved_keymap_preset = keymap_preset;
        self.selected_keymap_preset = keymap_preset;
    }

    pub fn discard_selection(&mut self) {
        self.selected_theme = self.previous_theme;
        self.selected_keymap_preset = self.saved_keymap_preset;
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

        state.select_next();
        assert_eq!(state.selected_theme(), ThemeId::Light);
        state.select_previous();
        assert_eq!(state.selected_theme(), ThemeId::Default);
    }

    #[test]
    fn selection_moves_between_keymap_presets() {
        let mut state = SettingsState::default();
        state.open(ThemeId::Default);
        state.switch_next_section();

        state.select_next();
        assert_eq!(state.selected_keymap_preset(), KeymapPreset::Ide);
        state.select_previous();
        assert_eq!(state.selected_keymap_preset(), KeymapPreset::Default);
    }

    #[test]
    fn loaded_keymap_preset_is_selected_on_open() {
        let mut state = SettingsState::default();

        state.load_keymap_preset(KeymapPreset::Ide);
        state.open(ThemeId::Default);

        assert_eq!(state.saved_keymap_preset(), KeymapPreset::Ide);
        assert_eq!(state.selected_keymap_preset(), KeymapPreset::Ide);
    }

    #[test]
    fn discard_selection_returns_to_previous_theme() {
        let mut state = SettingsState::default();
        state.open(ThemeId::Default);
        state.select_next();

        state.discard_selection();

        assert_eq!(state.selected_theme(), ThemeId::Default);
    }

    #[test]
    fn discard_selection_returns_to_saved_keymap_preset() {
        let mut state = SettingsState::default();
        state.load_keymap_preset(KeymapPreset::Ide);
        state.open(ThemeId::Default);
        state.select_previous();

        state.discard_selection();

        assert_eq!(state.selected_keymap_preset(), KeymapPreset::Ide);
    }
}
