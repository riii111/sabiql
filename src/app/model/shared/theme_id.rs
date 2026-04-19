#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(
    clippy::manual_non_exhaustive,
    reason = "hidden test-only palette variant keeps snapshot coverage without widening the runtime API"
)]
pub enum ThemeId {
    #[default]
    Default,
    #[doc(hidden)]
    TestContrast,
}
