#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum Mode {
    #[default]
    Browse,
}

use super::focused_pane::FocusedPane;

impl Mode {
    pub fn default_pane(self) -> FocusedPane {
        FocusedPane::Explorer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pane_returns_explorer() {
        assert_eq!(Mode::Browse.default_pane(), FocusedPane::Explorer);
    }
}
