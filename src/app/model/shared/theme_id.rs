#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeId {
    #[default]
    Default,
    Light,
}

impl ThemeId {
    pub const ALL: [Self; 2] = [Self::Default, Self::Light];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Sabiql Dark",
            Self::Light => "Light",
        }
    }

    pub const fn config_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Light => "light",
        }
    }

    pub fn from_config_value(value: &str) -> Option<Self> {
        match value {
            "default" | "sabiql-dark" | "dark" => Some(Self::Default),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|theme| *theme == self)
            .unwrap_or_default();
        Self::ALL[(index + 1).min(Self::ALL.len() - 1)]
    }

    pub fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|theme| *theme == self)
            .unwrap_or_default();
        Self::ALL[index.saturating_sub(1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_label_is_sabiql_dark() {
        assert_eq!(ThemeId::Default.label(), "Sabiql Dark");
    }

    #[test]
    fn config_value_round_trips_known_values() {
        for theme in ThemeId::ALL {
            assert_eq!(
                ThemeId::from_config_value(theme.config_value()),
                Some(theme)
            );
        }
    }

    #[test]
    fn unknown_config_value_returns_none() {
        assert_eq!(ThemeId::from_config_value("terminal"), None);
    }

    #[test]
    fn next_and_previous_clamp_at_edges() {
        assert_eq!(ThemeId::Default.previous(), ThemeId::Default);
        assert_eq!(ThemeId::Default.next(), ThemeId::Light);
        assert_eq!(ThemeId::Light.next(), ThemeId::Light);
        assert_eq!(ThemeId::Light.previous(), ThemeId::Default);
    }
}
