#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Schema {
    pub name: String,
}

impl Schema {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}
