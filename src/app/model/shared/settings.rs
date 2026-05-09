use super::text_input::TextInputState;
use super::theme_id::ThemeId;
use crate::update::action::CursorMove;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    ErDiagram,
}

impl SettingsSection {
    pub const ALL: [Self; 2] = [Self::Appearance, Self::ErDiagram];

    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::ErDiagram => "ER Diagram",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErBrowserChoice {
    SystemDefault,
    GoogleChrome,
    Firefox,
    Safari,
    MicrosoftEdge,
    Brave,
    Custom,
}

impl ErBrowserChoice {
    pub const ALL: [Self; 7] = [
        Self::SystemDefault,
        Self::GoogleChrome,
        Self::Firefox,
        Self::Safari,
        Self::MicrosoftEdge,
        Self::Brave,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SystemDefault => "System default",
            Self::GoogleChrome => "Google Chrome",
            Self::Firefox => "Firefox",
            Self::Safari => "Safari",
            Self::MicrosoftEdge => "Microsoft Edge",
            Self::Brave => "Brave",
            Self::Custom => "Custom",
        }
    }

    pub fn browser_name(self) -> Option<&'static str> {
        match self {
            Self::SystemDefault | Self::Custom => None,
            Self::GoogleChrome => Some("Google Chrome"),
            Self::Firefox => Some("Firefox"),
            Self::Safari => Some("Safari"),
            Self::MicrosoftEdge => Some("Microsoft Edge"),
            Self::Brave => Some("Brave"),
        }
    }

    pub fn from_browser_name(browser: Option<&str>) -> Self {
        match browser.map(str::trim).filter(|value| !value.is_empty()) {
            None => Self::SystemDefault,
            Some(
                "Google Chrome"
                | "google-chrome"
                | "google-chrome-stable"
                | "chromium"
                | "chromium-browser",
            ) => Self::GoogleChrome,
            Some("Firefox" | "firefox") => Self::Firefox,
            Some("Safari") => Self::Safari,
            Some("Microsoft Edge" | "microsoft-edge" | "microsoft-edge-stable") => {
                Self::MicrosoftEdge
            }
            Some("Brave" | "brave" | "brave-browser") => Self::Brave,
            Some(_) => Self::Custom,
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or(0);
        Self::ALL[(index + 1).min(Self::ALL.len() - 1)]
    }

    fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or(0);
        Self::ALL[index.saturating_sub(1)]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
    previous_theme: ThemeId,
    selected_theme: ThemeId,
    saved_er_browser: Option<String>,
    selected_er_browser_choice: ErBrowserChoice,
    custom_er_browser: TextInputState,
    editing_custom_er_browser: bool,
    section: SettingsSection,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            previous_theme: ThemeId::Default,
            selected_theme: ThemeId::Default,
            saved_er_browser: None,
            selected_er_browser_choice: ErBrowserChoice::SystemDefault,
            custom_er_browser: TextInputState::default(),
            editing_custom_er_browser: false,
            section: SettingsSection::Appearance,
        }
    }
}

impl SettingsState {
    pub fn load_er_browser(&mut self, er_browser: Option<String>) {
        self.saved_er_browser = normalize_browser(er_browser);
        self.selected_er_browser_choice =
            ErBrowserChoice::from_browser_name(self.saved_er_browser.as_deref());
        self.custom_er_browser = custom_input_for(self.saved_er_browser.as_deref());
    }

    pub fn open(&mut self, current_theme: ThemeId) {
        self.previous_theme = current_theme;
        self.selected_theme = current_theme;
        self.selected_er_browser_choice =
            ErBrowserChoice::from_browser_name(self.saved_er_browser.as_deref());
        self.custom_er_browser = custom_input_for(self.saved_er_browser.as_deref());
        self.editing_custom_er_browser = false;
        self.section = SettingsSection::Appearance;
    }

    pub fn previous_theme(&self) -> ThemeId {
        self.previous_theme
    }

    pub fn selected_theme(&self) -> ThemeId {
        self.selected_theme
    }

    pub fn section(&self) -> SettingsSection {
        self.section
    }

    pub fn saved_er_browser(&self) -> Option<&str> {
        self.saved_er_browser.as_deref()
    }

    pub fn selected_er_browser_choice(&self) -> ErBrowserChoice {
        self.selected_er_browser_choice
    }

    pub fn custom_er_browser(&self) -> &TextInputState {
        &self.custom_er_browser
    }

    pub fn is_editing_custom_er_browser(&self) -> bool {
        self.editing_custom_er_browser
            && self.section == SettingsSection::ErDiagram
            && self.selected_er_browser_choice == ErBrowserChoice::Custom
    }

    pub fn selected_er_browser(&self) -> Option<String> {
        match self.selected_er_browser_choice {
            ErBrowserChoice::SystemDefault => None,
            ErBrowserChoice::Custom => {
                normalize_browser(Some(self.custom_er_browser.content().trim().to_string()))
            }
            choice => choice.browser_name().map(str::to_string),
        }
    }

    pub fn switch_next_section(&mut self) {
        self.editing_custom_er_browser = false;
        self.section = self.section.next();
    }

    pub fn switch_previous_section(&mut self) {
        self.editing_custom_er_browser = false;
        self.section = self.section.previous();
    }

    pub fn select_next(&mut self) {
        match self.section {
            SettingsSection::Appearance => {
                self.selected_theme = self.selected_theme.next();
            }
            SettingsSection::ErDiagram => {
                self.editing_custom_er_browser = false;
                self.selected_er_browser_choice = self.selected_er_browser_choice.next();
            }
        }
    }

    pub fn select_previous(&mut self) {
        match self.section {
            SettingsSection::Appearance => {
                self.selected_theme = self.selected_theme.previous();
            }
            SettingsSection::ErDiagram => {
                self.editing_custom_er_browser = false;
                self.selected_er_browser_choice = self.selected_er_browser_choice.previous();
            }
        }
    }

    pub fn start_custom_browser_edit(&mut self) {
        if self.section == SettingsSection::ErDiagram {
            self.selected_er_browser_choice = ErBrowserChoice::Custom;
            self.editing_custom_er_browser = true;
        }
    }

    pub fn stop_custom_browser_edit(&mut self) {
        self.editing_custom_er_browser = false;
    }

    pub fn input_custom_browser(&mut self, ch: char) {
        if self.is_editing_custom_er_browser() {
            self.custom_er_browser.insert_char(ch);
        }
    }

    pub fn backspace_custom_browser(&mut self) {
        if self.is_editing_custom_er_browser() {
            self.custom_er_browser.backspace();
        }
    }

    pub fn delete_custom_browser(&mut self) {
        if self.is_editing_custom_er_browser() {
            self.custom_er_browser.delete();
        }
    }

    pub fn move_custom_browser_cursor(&mut self, direction: CursorMove) {
        if self.is_editing_custom_er_browser() {
            self.custom_er_browser.move_cursor(direction);
        }
    }

    pub fn commit_saved(&mut self, theme: ThemeId, er_browser: Option<String>) {
        self.previous_theme = theme;
        self.selected_theme = theme;
        self.saved_er_browser = normalize_browser(er_browser);
        self.selected_er_browser_choice =
            ErBrowserChoice::from_browser_name(self.saved_er_browser.as_deref());
        self.custom_er_browser = custom_input_for(self.saved_er_browser.as_deref());
        self.editing_custom_er_browser = false;
    }

    pub fn discard_selection(&mut self) {
        self.selected_theme = self.previous_theme;
        self.selected_er_browser_choice =
            ErBrowserChoice::from_browser_name(self.saved_er_browser.as_deref());
        self.custom_er_browser = custom_input_for(self.saved_er_browser.as_deref());
        self.editing_custom_er_browser = false;
    }
}

fn normalize_browser(browser: Option<String>) -> Option<String> {
    browser
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn custom_input_for(browser: Option<&str>) -> TextInputState {
    match browser {
        Some(value)
            if ErBrowserChoice::from_browser_name(Some(value)) == ErBrowserChoice::Custom =>
        {
            TextInputState::new(value, value.chars().count())
        }
        _ => TextInputState::default(),
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
    fn discard_selection_returns_to_previous_theme() {
        let mut state = SettingsState::default();
        state.open(ThemeId::Default);
        state.select_next();

        state.discard_selection();

        assert_eq!(state.selected_theme(), ThemeId::Default);
    }

    #[test]
    fn custom_browser_normalizes_empty_string_to_none() {
        let mut state = SettingsState::default();
        state.switch_next_section();
        state.start_custom_browser_edit();

        assert_eq!(state.selected_er_browser(), None);
    }

    #[test]
    fn known_browser_choice_returns_logical_browser_name() {
        let mut state = SettingsState::default();
        state.switch_next_section();
        state.select_next();

        assert_eq!(
            state.selected_er_browser().as_deref(),
            Some("Google Chrome")
        );
    }

    #[test]
    fn browser_choices_include_common_presets() {
        assert_eq!(
            ErBrowserChoice::ALL,
            [
                ErBrowserChoice::SystemDefault,
                ErBrowserChoice::GoogleChrome,
                ErBrowserChoice::Firefox,
                ErBrowserChoice::Safari,
                ErBrowserChoice::MicrosoftEdge,
                ErBrowserChoice::Brave,
                ErBrowserChoice::Custom,
            ]
        );
    }

    #[test]
    fn browser_choice_recognizes_command_aliases() {
        assert_eq!(
            ErBrowserChoice::from_browser_name(Some("chromium-browser")),
            ErBrowserChoice::GoogleChrome
        );
        assert_eq!(
            ErBrowserChoice::from_browser_name(Some("microsoft-edge-stable")),
            ErBrowserChoice::MicrosoftEdge
        );
        assert_eq!(
            ErBrowserChoice::from_browser_name(Some("brave-browser")),
            ErBrowserChoice::Brave
        );
    }

    #[test]
    fn loaded_custom_browser_populates_custom_input() {
        let mut state = SettingsState::default();

        state.load_er_browser(Some("Brave Browser".to_string()));
        state.open(ThemeId::Default);

        assert_eq!(state.selected_er_browser_choice(), ErBrowserChoice::Custom);
        assert_eq!(state.custom_er_browser().content(), "Brave Browser");
    }

    #[test]
    fn cursor_move_does_not_switch_preset_to_custom() {
        let mut state = SettingsState::default();
        state.switch_next_section();
        state.select_next();

        state.move_custom_browser_cursor(CursorMove::Left);

        assert_eq!(
            state.selected_er_browser_choice(),
            ErBrowserChoice::GoogleChrome
        );
    }

    #[test]
    fn custom_browser_input_requires_edit_mode() {
        let mut state = SettingsState::default();
        state.switch_next_section();
        state.select_previous();

        state.input_custom_browser('f');

        assert!(!state.is_editing_custom_er_browser());
        assert_eq!(state.custom_er_browser().content(), "");
    }
}
