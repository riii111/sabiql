#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum Mode {
    #[default]
    Browse,
    ER,
}

impl Mode {
    /// Convert tab index to Mode (0 = Browse, 1 = ER)
    pub fn from_tab_index(index: usize) -> Self {
        match index {
            0 => Mode::Browse,
            1 => Mode::ER,
            _ => Mode::Browse, // fallback
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
    #[case(2, Mode::Browse)] // fallback
    #[case(99, Mode::Browse)] // fallback
    fn from_tab_index_returns_correct_mode(#[case] index: usize, #[case] expected: Mode) {
        assert_eq!(Mode::from_tab_index(index), expected);
    }
}
