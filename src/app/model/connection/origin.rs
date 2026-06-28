#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionOrigin {
    #[default]
    Profile,
    CliEphemeral,
}

impl ConnectionOrigin {
    pub fn is_ephemeral(&self) -> bool {
        matches!(self, Self::CliEphemeral)
    }
}
