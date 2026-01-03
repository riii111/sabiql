use color_eyre::eyre::Result;

/// Callback pattern ensures terminal is always restored, even on panic.
pub trait TuiSession {
    fn with_suspended<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce() -> R;
}
