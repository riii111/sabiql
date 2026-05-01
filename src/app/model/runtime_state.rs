use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    project_name: String,
    service_file_path: Option<PathBuf>,
}

impl RuntimeState {
    pub fn new(project_name: String) -> Self {
        Self {
            project_name,
            service_file_path: None,
        }
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn service_file_path(&self) -> Option<&PathBuf> {
        self.service_file_path.as_ref()
    }

    pub fn set_service_file_path(&mut self, path: Option<PathBuf>) {
        self.service_file_path = path;
    }

    #[cfg(test)]
    pub(crate) fn set_project_name_for_test(&mut self, project_name: String) {
        self.project_name = project_name;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_runtime_state_with_project_name() {
        let state = RuntimeState::new("my_project".to_string());

        assert_eq!(state.project_name(), "my_project");
    }

    #[test]
    fn default_creates_empty_runtime_state() {
        let state = RuntimeState::default();

        assert!(state.project_name().is_empty());
    }
}
