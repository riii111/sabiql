#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    any(test, feature = "test-support"),
    allow(
        clippy::manual_non_exhaustive,
        reason = "hidden test-only palette variant keeps snapshot coverage without widening the runtime API"
    )
)]
pub enum ThemeId {
    #[default]
    Default,
    #[cfg(any(test, feature = "test-support"))]
    TestContrast,
}
