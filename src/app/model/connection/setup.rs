use std::collections::HashMap;

use crate::app::model::shared::text_input::TextInputState;
use crate::domain::connection::{ConnectionId, ConnectionProfile, SslMode};

pub const CONNECTION_INPUT_WIDTH: u16 = 30;
pub const CONNECTION_INPUT_VISIBLE_WIDTH: usize = (CONNECTION_INPUT_WIDTH - 4) as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionField {
    Name,
    Host,
    Port,
    Database,
    User,
    Password,
    SslMode,
}

impl ConnectionField {
    pub fn all() -> &'static [Self] {
        &[
            Self::Name,
            Self::Host,
            Self::Port,
            Self::Database,
            Self::User,
            Self::Password,
            Self::SslMode,
        ]
    }

    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Name => Some(Self::Host),
            Self::Host => Some(Self::Port),
            Self::Port => Some(Self::Database),
            Self::Database => Some(Self::User),
            Self::User => Some(Self::Password),
            Self::Password => Some(Self::SslMode),
            Self::SslMode => None,
        }
    }

    pub fn prev(&self) -> Option<Self> {
        match self {
            Self::Name => None,
            Self::Host => Some(Self::Name),
            Self::Port => Some(Self::Host),
            Self::Database => Some(Self::Port),
            Self::User => Some(Self::Database),
            Self::Password => Some(Self::User),
            Self::SslMode => Some(Self::Password),
        }
    }

    pub fn is_required(&self) -> bool {
        matches!(
            self,
            Self::Name | Self::Host | Self::Port | Self::Database | Self::User
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Name => "Name:",
            Self::Host => "Host:",
            Self::Port => "Port:",
            Self::Database => "Database:",
            Self::User => "User:",
            Self::Password => "Password:",
            Self::SslMode => "SSL Mode:",
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
    pub name: TextInputState,
    pub host: TextInputState,
    pub port: TextInputState,
    pub database: TextInputState,
    pub user: TextInputState,
    pub password: TextInputState,
    pub ssl_mode: SslMode,

    pub focused_field: ConnectionField,
    pub ssl_dropdown: SslModeDropdown,
    pub validation_errors: HashMap<ConnectionField, String>,

    pub is_first_run: bool,

    pub editing_id: Option<ConnectionId>,
}

impl Default for ConnectionSetupState {
    fn default() -> Self {
        Self {
            name: TextInputState::default(),
            host: TextInputState::new("localhost", 9),
            port: TextInputState::new("5432", 4),
            database: TextInputState::default(),
            user: TextInputState::default(),
            password: TextInputState::default(),
            ssl_mode: SslMode::Prefer,
            focused_field: ConnectionField::Name,
            ssl_dropdown: SslModeDropdown::default(),
            validation_errors: HashMap::new(),
            is_first_run: true,
            editing_id: None,
        }
    }
}

impl ConnectionSetupState {
    pub fn default_name(&self) -> String {
        if self.database.content().is_empty() {
            self.host.content().to_string()
        } else {
            format!("{}@{}", self.database.content(), self.host.content())
        }
    }

    pub fn field_value(&self, field: ConnectionField) -> &str {
        match field {
            ConnectionField::Name => self.name.content(),
            ConnectionField::Host => self.host.content(),
            ConnectionField::Port => self.port.content(),
            ConnectionField::Database => self.database.content(),
            ConnectionField::User => self.user.content(),
            ConnectionField::Password => self.password.content(),
            ConnectionField::SslMode => "",
        }
    }

    pub fn focused_input(&self) -> Option<&TextInputState> {
        match self.focused_field {
            ConnectionField::Name => Some(&self.name),
            ConnectionField::Host => Some(&self.host),
            ConnectionField::Port => Some(&self.port),
            ConnectionField::Database => Some(&self.database),
            ConnectionField::User => Some(&self.user),
            ConnectionField::Password => Some(&self.password),
            ConnectionField::SslMode => None,
        }
    }

    pub fn focused_input_mut(&mut self) -> Option<&mut TextInputState> {
        match self.focused_field {
            ConnectionField::Name => Some(&mut self.name),
            ConnectionField::Host => Some(&mut self.host),
            ConnectionField::Port => Some(&mut self.port),
            ConnectionField::Database => Some(&mut self.database),
            ConnectionField::User => Some(&mut self.user),
            ConnectionField::Password => Some(&mut self.password),
            ConnectionField::SslMode => None,
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

    pub fn from_profile(profile: &ConnectionProfile) -> Self {
        let name_len = profile.name.as_str().chars().count();
        let host_len = profile.host.chars().count();
        let port_str = profile.port.to_string();
        let port_len = port_str.chars().count();
        let db_len = profile.database.chars().count();
        let user_len = profile.username.chars().count();
        let pw_len = profile.password.chars().count();
        Self {
            name: TextInputState::new(profile.name.as_str(), name_len),
            host: TextInputState::new(&profile.host, host_len),
            port: TextInputState::new(&port_str, port_len),
            database: TextInputState::new(&profile.database, db_len),
            user: TextInputState::new(&profile.username, user_len),
            password: TextInputState::new(&profile.password, pw_len),
            ssl_mode: profile.ssl_mode,
            focused_field: ConnectionField::Name,
            ssl_dropdown: SslModeDropdown::default(),
            validation_errors: HashMap::new(),
            is_first_run: false,
            editing_id: Some(profile.id.clone()),
        }
    }

    pub fn is_edit_mode(&self) -> bool {
        self.editing_id.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod connection_field {
        use super::*;

        #[rstest]
        #[case(ConnectionField::Name, Some(ConnectionField::Host))]
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
        #[case(ConnectionField::Name, None)]
        #[case(ConnectionField::Host, Some(ConnectionField::Name))]
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
        #[case(ConnectionField::Name, true)]
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
            assert_eq!(all.len(), 7);
            assert_eq!(all[0], ConnectionField::Name);
            assert_eq!(all[6], ConnectionField::SslMode);
        }
    }

    mod connection_setup_state {
        use super::*;

        #[test]
        fn default_has_correct_values() {
            let state = ConnectionSetupState::default();
            assert!(state.name.content().is_empty());
            assert_eq!(state.host.content(), "localhost");
            assert_eq!(state.port.content(), "5432");
            assert!(state.database.content().is_empty());
            assert!(state.user.content().is_empty());
            assert!(state.password.content().is_empty());
            assert_eq!(state.ssl_mode, SslMode::Prefer);
            assert_eq!(state.focused_field, ConnectionField::Name);
            assert!(state.is_first_run);
            assert!(state.editing_id.is_none());
        }

        #[test]
        fn default_name_without_database() {
            let state = ConnectionSetupState::default();
            assert_eq!(state.default_name(), "localhost");
        }

        #[test]
        fn default_name_with_database() {
            let mut state = ConnectionSetupState::default();
            state.database.set_content("mydb".to_string());
            assert_eq!(state.default_name(), "mydb@localhost");
        }

        #[test]
        fn has_errors_returns_false_when_empty() {
            let state = ConnectionSetupState::default();
            assert!(!state.has_errors());
        }

        #[test]
        fn has_errors_returns_true_when_errors_exist() {
            let mut state = ConnectionSetupState::default();
            state
                .validation_errors
                .insert(ConnectionField::Host, "Required".to_string());
            assert!(state.has_errors());
        }

        #[test]
        fn clear_errors_removes_all_errors() {
            let mut state = ConnectionSetupState::default();
            state
                .validation_errors
                .insert(ConnectionField::Host, "Required".to_string());
            state
                .validation_errors
                .insert(ConnectionField::Port, "Invalid".to_string());
            state.clear_errors();
            assert!(!state.has_errors());
        }

        #[test]
        fn from_profile_populates_all_fields() {
            let profile = ConnectionProfile::new(
                "Test DB",
                "db.example.com",
                5433,
                "testdb",
                "testuser",
                "secret",
                SslMode::Require,
            )
            .unwrap();

            let state = ConnectionSetupState::from_profile(&profile);

            assert_eq!(state.name.content(), "Test DB");
            assert_eq!(state.host.content(), "db.example.com");
            assert_eq!(state.port.content(), "5433");
            assert_eq!(state.database.content(), "testdb");
            assert_eq!(state.user.content(), "testuser");
            assert_eq!(state.password.content(), "secret");
            assert_eq!(state.ssl_mode, SslMode::Require);
            assert_eq!(state.editing_id, Some(profile.id));
            assert!(!state.is_first_run);
        }

        #[test]
        fn is_edit_mode_returns_false_for_new() {
            let state = ConnectionSetupState::default();
            assert!(!state.is_edit_mode());
        }

        #[test]
        fn is_edit_mode_returns_true_for_edit() {
            let profile = ConnectionProfile::new(
                "Test",
                "localhost",
                5432,
                "db",
                "user",
                "",
                SslMode::Prefer,
            )
            .unwrap();
            let state = ConnectionSetupState::from_profile(&profile);
            assert!(state.is_edit_mode());
        }

        #[test]
        fn focused_input_returns_correct_field() {
            let state = ConnectionSetupState {
                focused_field: ConnectionField::Host,
                ..Default::default()
            };
            assert!(state.focused_input().is_some());
            assert_eq!(state.focused_input().unwrap().content(), "localhost");
        }

        #[test]
        fn focused_input_returns_none_for_ssl() {
            let state = ConnectionSetupState {
                focused_field: ConnectionField::SslMode,
                ..Default::default()
            };
            assert!(state.focused_input().is_none());
        }
    }
}
