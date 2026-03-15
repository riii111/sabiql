use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    pub project_name: String,
    pub service_file_path: Option<PathBuf>,
}

impl RuntimeState {
    pub fn new(project_name: String) -> Self {
        Self {
            project_name,
            service_file_path: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_runtime_state_with_project_name() {
        let state = RuntimeState::new("my_project".to_string());

        assert_eq!(state.project_name, "my_project");
    }

    #[test]
    fn default_creates_empty_runtime_state() {
        let state = RuntimeState::default();

        assert!(state.project_name.is_empty());
    }
}
