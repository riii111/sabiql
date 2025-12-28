#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum Mode {
    #[default]
    Browse,
    ER,
}

use super::focused_pane::FocusedPane;

impl Mode {
    pub fn from_tab_index(index: usize) -> Self {
        debug_assert!(index < 2, "Invalid tab index: {}", index);
        match index {
            0 => Mode::Browse,
            1 => Mode::ER,
            _ => Mode::Browse,
        }
    }

    pub fn default_pane(self) -> FocusedPane {
        match self {
            Mode::Browse => FocusedPane::Explorer,
            // TODO: Change to Graph once ER view is implemented
            Mode::ER => FocusedPane::Explorer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, Mode::Browse)]
    #[case(1, Mode::ER)]
    fn from_tab_index_returns_correct_mode(#[case] index: usize, #[case] expected: Mode) {
        assert_eq!(Mode::from_tab_index(index), expected);
    }

    #[rstest]
    #[case(Mode::Browse, FocusedPane::Explorer)]
    #[case(Mode::ER, FocusedPane::Explorer)] // TODO: Change to Graph once ER view is implemented
    fn default_pane_returns_correct_pane(#[case] mode: Mode, #[case] expected: FocusedPane) {
        assert_eq!(mode.default_pane(), expected);
    }
}
