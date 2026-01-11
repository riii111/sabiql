use super::connection_state::ConnectionState;

#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    pub dsn: Option<String>,
    pub project_name: String,
    pub profile_name: String,
    pub database_name: Option<String>,
    pub active_connection_name: Option<String>,
    pub connection_state: ConnectionState,
    pub is_reconnecting: bool,
}

impl RuntimeState {
    pub fn new(project_name: String, profile_name: String) -> Self {
        Self {
            dsn: None,
            project_name,
            profile_name,
            database_name: None,
            active_connection_name: None,
            connection_state: ConnectionState::default(),
            is_reconnecting: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_runtime_state_with_names() {
        let state = RuntimeState::new("my_project".to_string(), "production".to_string());

        assert_eq!(state.project_name, "my_project");
        assert_eq!(state.profile_name, "production");
        assert!(state.dsn.is_none());
        assert!(state.database_name.is_none());
        assert!(state.active_connection_name.is_none());
        assert!(state.connection_state.is_not_connected());
        assert!(!state.is_reconnecting);
    }

    #[test]
    fn default_creates_empty_runtime_state() {
        let state = RuntimeState::default();

        assert!(state.project_name.is_empty());
        assert!(state.profile_name.is_empty());
        assert!(state.dsn.is_none());
        assert!(state.database_name.is_none());
        assert!(state.active_connection_name.is_none());
        assert!(state.connection_state.is_not_connected());
        assert!(!state.is_reconnecting);
    }
}
