use std::collections::HashMap;

use crate::domain::connection::SslMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionField {
    Host,
    Port,
    Database,
    User,
    Password,
    SslMode,
}

impl ConnectionField {
    pub fn all() -> &'static [ConnectionField] {
        &[
            ConnectionField::Host,
            ConnectionField::Port,
            ConnectionField::Database,
            ConnectionField::User,
            ConnectionField::Password,
            ConnectionField::SslMode,
        ]
    }

    pub fn next(&self) -> Option<ConnectionField> {
        match self {
            ConnectionField::Host => Some(ConnectionField::Port),
            ConnectionField::Port => Some(ConnectionField::Database),
            ConnectionField::Database => Some(ConnectionField::User),
            ConnectionField::User => Some(ConnectionField::Password),
            ConnectionField::Password => Some(ConnectionField::SslMode),
            ConnectionField::SslMode => None,
        }
    }

    pub fn prev(&self) -> Option<ConnectionField> {
        match self {
            ConnectionField::Host => None,
            ConnectionField::Port => Some(ConnectionField::Host),
            ConnectionField::Database => Some(ConnectionField::Port),
            ConnectionField::User => Some(ConnectionField::Database),
            ConnectionField::Password => Some(ConnectionField::User),
            ConnectionField::SslMode => Some(ConnectionField::Password),
        }
    }

    pub fn is_required(&self) -> bool {
        matches!(
            self,
            ConnectionField::Host
                | ConnectionField::Port
                | ConnectionField::Database
                | ConnectionField::User
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            ConnectionField::Host => "Host:",
            ConnectionField::Port => "Port:",
            ConnectionField::Database => "Database:",
            ConnectionField::User => "User:",
            ConnectionField::Password => "Password:",
            ConnectionField::SslMode => "SSL Mode:",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SslModeDropdown {
    pub is_open: bool,
    pub selected_index: usize,
}

#[derive(Debug, Clone)]
pub struct ConnectionSetupState {
    pub host: String,
    pub port: String,
    pub database: String,
    pub user: String,
    pub password: String,
    pub ssl_mode: SslMode,

    pub focused_field: ConnectionField,
    pub ssl_dropdown: SslModeDropdown,
    pub validation_errors: HashMap<ConnectionField, String>,

    pub cursor_position: usize,
    pub viewport_offset: usize,

    /// Determines cancel behavior: first run shows quit confirmation, otherwise returns to Normal
    pub is_first_run: bool,
}

impl Default for ConnectionSetupState {
    fn default() -> Self {
        let host = "localhost".to_string();
        let cursor_position = host.len();
        Self {
            host,
            port: "5432".to_string(),
            database: String::new(),
            user: String::new(),
            password: String::new(),
            ssl_mode: SslMode::Prefer,
            focused_field: ConnectionField::Host,
            ssl_dropdown: SslModeDropdown::default(),
            validation_errors: HashMap::new(),
            cursor_position,
            viewport_offset: 0,
            is_first_run: true,
        }
    }
}

impl ConnectionSetupState {
    /// Format: host:port/database (matches ConnectionProfile::display_name)
    pub fn auto_name(&self) -> String {
        if self.database.is_empty() {
            format!("{}:{}", self.host, self.port)
        } else {
            format!("{}:{}/{}", self.host, self.port, self.database)
        }
    }

    pub fn field_value(&self, field: ConnectionField) -> &str {
        match field {
            ConnectionField::Host => &self.host,
            ConnectionField::Port => &self.port,
            ConnectionField::Database => &self.database,
            ConnectionField::User => &self.user,
            ConnectionField::Password => &self.password,
            ConnectionField::SslMode => "",
        }
    }

    pub fn clear_errors(&mut self) {
        self.validation_errors.clear();
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn has_errors(&self) -> bool {
        !self.validation_errors.is_empty()
    }

    pub fn update_cursor(&mut self, cursor: usize, visible_width: usize) {
        self.cursor_position = cursor;
        if cursor < self.viewport_offset {
            self.viewport_offset = cursor;
        } else if cursor >= self.viewport_offset + visible_width {
            self.viewport_offset = cursor.saturating_sub(visible_width) + 1;
        }
    }

    pub fn cursor_to_end(&mut self) {
        let len = self.field_value(self.focused_field).len();
        self.cursor_position = len;
        self.viewport_offset = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod connection_field {
        use super::*;

        #[rstest]
        #[case(ConnectionField::Host, Some(ConnectionField::Port))]
        #[case(ConnectionField::Port, Some(ConnectionField::Database))]
        #[case(ConnectionField::Database, Some(ConnectionField::User))]
        #[case(ConnectionField::User, Some(ConnectionField::Password))]
        #[case(ConnectionField::Password, Some(ConnectionField::SslMode))]
        #[case(ConnectionField::SslMode, None)]
        fn next_returns_correct_field(
            #[case] field: ConnectionField,
            #[case] expected: Option<ConnectionField>,
        ) {
            assert_eq!(field.next(), expected);
        }

        #[rstest]
        #[case(ConnectionField::Host, None)]
        #[case(ConnectionField::Port, Some(ConnectionField::Host))]
        #[case(ConnectionField::Database, Some(ConnectionField::Port))]
        #[case(ConnectionField::User, Some(ConnectionField::Database))]
        #[case(ConnectionField::Password, Some(ConnectionField::User))]
        #[case(ConnectionField::SslMode, Some(ConnectionField::Password))]
        fn prev_returns_correct_field(
            #[case] field: ConnectionField,
            #[case] expected: Option<ConnectionField>,
        ) {
            assert_eq!(field.prev(), expected);
        }

        #[rstest]
        #[case(ConnectionField::Host, true)]
        #[case(ConnectionField::Port, true)]
        #[case(ConnectionField::Database, true)]
        #[case(ConnectionField::User, true)]
        #[case(ConnectionField::Password, false)]
        #[case(ConnectionField::SslMode, false)]
        fn is_required_returns_correct_value(
            #[case] field: ConnectionField,
            #[case] expected: bool,
        ) {
            assert_eq!(field.is_required(), expected);
        }

        #[test]
        fn all_returns_fields_in_order() {
            let all = ConnectionField::all();
            assert_eq!(all.len(), 6);
            assert_eq!(all[0], ConnectionField::Host);
            assert_eq!(all[5], ConnectionField::SslMode);
        }
    }

    mod connection_setup_state {
        use super::*;

        #[test]
        fn default_has_correct_values() {
            let state = ConnectionSetupState::default();
            assert_eq!(state.host, "localhost");
            assert_eq!(state.port, "5432");
            assert!(state.database.is_empty());
            assert!(state.user.is_empty());
            assert!(state.password.is_empty());
            assert_eq!(state.ssl_mode, SslMode::Prefer);
            assert_eq!(state.focused_field, ConnectionField::Host);
            assert!(state.is_first_run);
        }

        #[test]
        fn auto_name_without_database() {
            let state = ConnectionSetupState::default();
            assert_eq!(state.auto_name(), "localhost:5432");
        }

        #[test]
        fn auto_name_with_database() {
            let state = ConnectionSetupState {
                database: "mydb".to_string(),
                ..Default::default()
            };
            assert_eq!(state.auto_name(), "localhost:5432/mydb");
        }

        #[test]
        fn has_errors_returns_false_when_empty() {
            let state = ConnectionSetupState::default();
            assert!(!state.has_errors());
        }

        #[test]
        fn has_errors_returns_true_when_errors_exist() {
            let state = ConnectionSetupState {
                validation_errors: HashMap::from([(ConnectionField::Host, "Required".to_string())]),
                ..Default::default()
            };
            assert!(state.has_errors());
        }

        #[test]
        fn clear_errors_removes_all_errors() {
            let mut state = ConnectionSetupState {
                validation_errors: HashMap::from([
                    (ConnectionField::Host, "Required".to_string()),
                    (ConnectionField::Port, "Invalid".to_string()),
                ]),
                ..Default::default()
            };
            state.clear_errors();
            assert!(!state.has_errors());
        }
    }
}
