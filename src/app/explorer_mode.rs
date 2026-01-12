//! Explorer pane display mode.

/// Controls what the Explorer pane displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerMode {
    /// Display database tables (default).
    #[default]
    Tables,
    /// Display saved connections.
    Connections,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_tables() {
        let mode = ExplorerMode::default();

        assert_eq!(mode, ExplorerMode::Tables);
    }

    #[test]
    fn modes_are_distinct() {
        assert_ne!(ExplorerMode::Tables, ExplorerMode::Connections);
    }
}
