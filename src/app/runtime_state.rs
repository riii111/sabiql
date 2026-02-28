use std::path::PathBuf;

use super::connection_state::ConnectionState;
use crate::domain::ConnectionId;

#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    pub dsn: Option<String>,
    pub project_name: String,
    pub database_name: Option<String>,
    pub active_connection_id: Option<ConnectionId>,
    pub active_connection_name: Option<String>,
    pub connection_state: ConnectionState,
    pub is_reloading: bool,
    pub service_file_path: Option<PathBuf>,
}

impl RuntimeState {
    pub fn new(project_name: String) -> Self {
        Self {
            dsn: None,
            project_name,
            database_name: None,
            active_connection_id: None,
            active_connection_name: None,
            connection_state: ConnectionState::default(),
            is_reloading: false,
            service_file_path: None,
        }
    }

    pub fn is_service_connection(&self) -> bool {
        self.dsn.as_ref().is_some_and(|d| d.starts_with("service="))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_runtime_state_with_project_name() {
        let state = RuntimeState::new("my_project".to_string());

        assert_eq!(state.project_name, "my_project");
        assert!(state.dsn.is_none());
        assert!(state.database_name.is_none());
        assert!(state.active_connection_id.is_none());
        assert!(state.active_connection_name.is_none());
        assert!(state.connection_state.is_not_connected());
        assert!(!state.is_reloading);
    }

    #[test]
    fn default_creates_empty_runtime_state() {
        let state = RuntimeState::default();

        assert!(state.project_name.is_empty());
        assert!(state.dsn.is_none());
        assert!(state.database_name.is_none());
        assert!(state.active_connection_id.is_none());
        assert!(state.active_connection_name.is_none());
        assert!(state.connection_state.is_not_connected());
        assert!(!state.is_reloading);
    }
}
