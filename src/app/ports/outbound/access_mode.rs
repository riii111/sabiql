#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

impl AccessMode {
    pub fn from_read_only(read_only: bool) -> Self {
        if read_only {
            Self::ReadOnly
        } else {
            Self::ReadWrite
        }
    }

    pub fn is_read_only(self) -> bool {
        matches!(self, Self::ReadOnly)
    }
}
