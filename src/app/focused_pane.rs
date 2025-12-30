#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPane {
    // Browse mode panes
    #[default]
    Explorer,
    Inspector,
    Result,
    // ER mode panes
    Graph,
    Details,
}

impl FocusedPane {
    #[allow(dead_code)]
    pub fn browse_default() -> Self {
        Self::Explorer
    }

    #[allow(dead_code)]
    pub fn er_default() -> Self {
        Self::Graph
    }

    pub fn from_browse_key(key: char) -> Option<Self> {
        match key {
            '1' => Some(Self::Explorer),
            '2' => Some(Self::Inspector),
            '3' => Some(Self::Result),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn from_er_key(key: char) -> Option<Self> {
        match key {
            '1' => Some(Self::Graph),
            '2' => Some(Self::Details),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn default_is_explorer() {
        assert_eq!(FocusedPane::default(), FocusedPane::Explorer);
    }

    #[rstest]
    #[case('1', FocusedPane::Explorer)]
    #[case('2', FocusedPane::Inspector)]
    #[case('3', FocusedPane::Result)]
    fn from_browse_key_returns_correct_pane(#[case] key: char, #[case] expected: FocusedPane) {
        assert_eq!(FocusedPane::from_browse_key(key), Some(expected));
    }

    #[rstest]
    #[case('4')]
    #[case('0')]
    #[case('a')]
    fn from_browse_key_returns_none_for_invalid(#[case] key: char) {
        assert_eq!(FocusedPane::from_browse_key(key), None);
    }

    #[rstest]
    #[case('1', FocusedPane::Graph)]
    #[case('2', FocusedPane::Details)]
    fn from_er_key_returns_correct_pane(#[case] key: char, #[case] expected: FocusedPane) {
        assert_eq!(FocusedPane::from_er_key(key), Some(expected));
    }

    #[rstest]
    #[case('3')]
    #[case('0')]
    fn from_er_key_returns_none_for_invalid(#[case] key: char) {
        assert_eq!(FocusedPane::from_er_key(key), None);
    }
}
