#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeId {
    #[default]
    Default,
    #[cfg(any(test, feature = "test-support"))]
    TestContrast,
}
