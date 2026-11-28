use super::theme_id::ThemeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Keymap,
    Icons,
}

impl SettingsSection {
    pub const ALL: [Self; 3] = [Self::Appearance, Self::Keymap, Self::Icons];

    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Keymap => "Keymap",
            Self::Icons => "Icons",
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
pub enum IconMode {
    Off,
    NerdFont,
    #[default]
    Unicode,
    Ascii,
}

impl IconMode {
    pub const ALL: [Self; 4] = [Self::Off, Self::NerdFont, Self::Unicode, Self::Ascii];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::NerdFont => "Nerd Font",
            Self::Unicode => "Unicode",
            Self::Ascii => "ASCII",
        }
    }

    pub fn config_value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::NerdFont => "nerd-font",
            Self::Unicode => "unicode",
            Self::Ascii => "ascii",
        }
    }

    pub fn from_config_value(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.config_value() == value)
    }

    fn step(self, delta: isize) -> Self {
        let index = Self::ALL
            .iter()
            .position(|mode| *mode == self)
            .unwrap_or_default();
        Self::ALL[index.saturating_add_signed(delta).min(Self::ALL.len() - 1)]
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
    saved_icon_mode: IconMode,
    selected_icon_mode: IconMode,
    saved_keymap_preset: KeymapPreset,
    selected_keymap_preset: KeymapPreset,
    section: SettingsSection,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            previous_theme: ThemeId::Default,
            selected_theme: ThemeId::Default,
            saved_icon_mode: IconMode::default(),
            selected_icon_mode: IconMode::default(),
            saved_keymap_preset: KeymapPreset::Default,
            selected_keymap_preset: KeymapPreset::Default,
            section: SettingsSection::Appearance,
        }
    }
}

impl SettingsState {
    pub fn load_icon_mode(&mut self, mode: IconMode) {
        self.saved_icon_mode = mode;
        self.selected_icon_mode = mode;
    }

    pub fn saved_icon_mode(&self) -> IconMode {
        self.saved_icon_mode
    }

    pub fn selected_icon_mode(&self) -> IconMode {
        self.selected_icon_mode
    }

    pub fn load_keymap_preset(&mut self, keymap_preset: KeymapPreset) {
        self.saved_keymap_preset = keymap_preset;
        self.selected_keymap_preset = keymap_preset;
    }

    pub fn open(&mut self, current_theme: ThemeId) {
        self.previous_theme = current_theme;
        self.selected_theme = current_theme;
        self.selected_icon_mode = self.saved_icon_mode;
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
            SettingsSection::Icons => self.selected_icon_mode = self.selected_icon_mode.step(1),
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
            SettingsSection::Icons => self.selected_icon_mode = self.selected_icon_mode.step(-1),
            SettingsSection::Keymap => {
                self.selected_keymap_preset = self.selected_keymap_preset.previous();
            }
        }
    }

    pub fn commit_saved(
        &mut self,
        theme: ThemeId,
        icon_mode: IconMode,
        keymap_preset: KeymapPreset,
    ) {
        // A save completes after later input can arrive, so a selection edited since the save is kept instead of being reset to the saved values.
        let edited_since_save = self.selected_theme != theme
            || self.selected_icon_mode != icon_mode
            || self.selected_keymap_preset != keymap_preset;
        self.previous_theme = theme;
        self.saved_icon_mode = icon_mode;
        self.saved_keymap_preset = keymap_preset;
        if !edited_since_save {
            self.discard_selection();
        }
    }

    pub fn discard_selection(&mut self) {
        self.selected_theme = self.previous_theme;
        self.selected_icon_mode = self.saved_icon_mode;
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
