pub struct MySqlAdapter;

impl MySqlAdapter {
    #[allow(
        clippy::new_without_default,
        reason = "new() is the only default construction API"
    )]
    pub fn new() -> Self {
        Self
    }
}
