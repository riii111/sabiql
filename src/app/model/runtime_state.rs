use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RuntimeState {
    pub(crate) project_name: String,
    pub(crate) service_file_path: Option<PathBuf>,
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

    pub fn service_file_path(&self) -> Option<&Path> {
        self.service_file_path.as_deref()
    }

    pub fn set_service_file_path(&mut self, path: Option<PathBuf>) {
        self.service_file_path = path;
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
}
