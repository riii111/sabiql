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
        match self {
            Self::Appearance => Self::ErDiagram,
            Self::ErDiagram => Self::Appearance,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Appearance => Self::ErDiagram,
            Self::ErDiagram => Self::Appearance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErBrowserChoice {
    SystemDefault,
    GoogleChrome,
    Firefox,
    Safari,
    Custom,
}

impl ErBrowserChoice {
    #[cfg(target_os = "macos")]
    pub const ALL: [Self; 5] = [
        Self::SystemDefault,
        Self::GoogleChrome,
        Self::Firefox,
        Self::Safari,
        Self::Custom,
    ];

    #[cfg(not(target_os = "macos"))]
    pub const ALL: [Self; 4] = [
        Self::SystemDefault,
        Self::GoogleChrome,
        Self::Firefox,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SystemDefault => "System default",
            Self::GoogleChrome => "Google Chrome",
            Self::Firefox => "Firefox",
            Self::Safari => "Safari",
            Self::Custom => "Custom",
        }
    }

    pub fn browser_name(self) -> Option<&'static str> {
        match self {
            Self::SystemDefault | Self::Custom => None,
            #[cfg(target_os = "macos")]
            Self::GoogleChrome => Some("Google Chrome"),
            #[cfg(not(target_os = "macos"))]
            Self::GoogleChrome => Some("google-chrome"),
            #[cfg(target_os = "macos")]
            Self::Firefox => Some("Firefox"),
            #[cfg(not(target_os = "macos"))]
            Self::Firefox => Some("firefox"),
            #[cfg(target_os = "macos")]
            Self::Safari => Some("Safari"),
            #[cfg(not(target_os = "macos"))]
            Self::Safari => None,
        }
    }

    pub fn from_browser_name(browser: Option<&str>) -> Self {
        match browser.map(str::trim).filter(|value| !value.is_empty()) {
            None => Self::SystemDefault,
            Some("Google Chrome" | "google-chrome" | "google-chrome-stable") => Self::GoogleChrome,
            Some("Firefox" | "firefox") => Self::Firefox,
            #[cfg(target_os = "macos")]
            Some("Safari") => Self::Safari,
            #[cfg(not(target_os = "macos"))]
            Some("Safari") => Self::Custom,
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
            SettingsSection::ErDiagram => {
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
                self.selected_er_browser_choice = self.selected_er_browser_choice.previous();
            }
        }
    }

    pub fn input_custom_browser(&mut self, ch: char) {
        self.selected_er_browser_choice = ErBrowserChoice::Custom;
        self.custom_er_browser.insert_char(ch);
    }

    pub fn backspace_custom_browser(&mut self) {
        self.selected_er_browser_choice = ErBrowserChoice::Custom;
        self.custom_er_browser.backspace();
    }

    pub fn delete_custom_browser(&mut self) {
        self.selected_er_browser_choice = ErBrowserChoice::Custom;
        self.custom_er_browser.delete();
    }

    pub fn move_custom_browser_cursor(&mut self, direction: CursorMove) {
        if self.selected_er_browser_choice == ErBrowserChoice::Custom {
            self.custom_er_browser.move_cursor(direction);
        }
    }

    pub fn commit_selection(&mut self) {
        self.previous_theme = self.selected_theme;
        self.saved_er_browser = self.selected_er_browser();
    }

    pub fn discard_selection(&mut self) {
        self.selected_theme = self.previous_theme;
        self.selected_er_browser_choice =
            ErBrowserChoice::from_browser_name(self.saved_er_browser.as_deref());
        self.custom_er_browser = custom_input_for(self.saved_er_browser.as_deref());
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
        state.select_previous();

        assert_eq!(state.selected_er_browser(), None);
    }

    #[test]
    fn known_browser_choice_returns_browser_name() {
        let mut state = SettingsState::default();
        state.switch_next_section();
        state.select_next();

        #[cfg(target_os = "macos")]
        let expected = "Google Chrome";
        #[cfg(not(target_os = "macos"))]
        let expected = "google-chrome";
        assert_eq!(state.selected_er_browser().as_deref(), Some(expected));
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
}
