use std::collections::HashMap;
use std::path::Path;

use crate::domain::connection::{
    ConnectionConfig, ConnectionId, ConnectionProfile, DatabaseType, SqliteConnectionConfigError,
    SslMode,
};
use crate::model::shared::text_input::TextInputState;

pub const CONNECTION_INPUT_WIDTH: u16 = 30;
pub const CONNECTION_INPUT_VISIBLE_WIDTH: usize = (CONNECTION_INPUT_WIDTH - 4) as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionField {
    DatabaseType,
    Name,
    SqlitePath,
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
            Self::DatabaseType,
            Self::Name,
            Self::SqlitePath,
            Self::Host,
            Self::Port,
            Self::Database,
            Self::User,
            Self::Password,
            Self::SslMode,
        ]
    }

    pub fn fields_for(database_type: DatabaseType) -> &'static [Self] {
        match database_type {
            DatabaseType::PostgreSQL => &[
                Self::DatabaseType,
                Self::Name,
                Self::Host,
                Self::Port,
                Self::Database,
                Self::User,
                Self::Password,
                Self::SslMode,
            ],
            DatabaseType::SQLite => &[Self::DatabaseType, Self::Name, Self::SqlitePath],
        }
    }

    pub fn is_required(self) -> bool {
        matches!(
            self,
            Self::Name | Self::SqlitePath | Self::Host | Self::Port | Self::Database | Self::User
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DatabaseType => "Type:",
            Self::Name => "Name:",
            Self::SqlitePath => "Path:",
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

#[derive(Debug, Clone, Default)]
pub struct DatabaseTypeDropdown {
    pub is_open: bool,
    pub selected_index: usize,
}

#[derive(Debug, Clone)]
pub struct ConnectionSetupState {
    pub database_type: DatabaseType,
    pub name: TextInputState,
    pub sqlite_path: TextInputState,
    pub host: TextInputState,
    pub port: TextInputState,
    pub database: TextInputState,
    pub user: TextInputState,
    pub password: TextInputState,
    pub ssl_mode: SslMode,

    pub focused_field: ConnectionField,
    pub database_type_dropdown: DatabaseTypeDropdown,
    pub ssl_dropdown: SslModeDropdown,
    pub validation_errors: HashMap<ConnectionField, String>,

    pub is_first_run: bool,

    pub editing_id: Option<ConnectionId>,
}

impl Default for ConnectionSetupState {
    fn default() -> Self {
        Self {
            database_type: DatabaseType::PostgreSQL,
            name: TextInputState::default(),
            sqlite_path: TextInputState::default(),
            host: TextInputState::new("localhost", 9),
            port: TextInputState::new("5432", 4),
            database: TextInputState::default(),
            user: TextInputState::default(),
            password: TextInputState::default(),
            ssl_mode: SslMode::Prefer,
            focused_field: ConnectionField::DatabaseType,
            database_type_dropdown: DatabaseTypeDropdown::default(),
            ssl_dropdown: SslModeDropdown::default(),
            validation_errors: HashMap::new(),
            is_first_run: true,
            editing_id: None,
        }
    }
}

impl ConnectionSetupState {
    pub fn default_name(&self) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                if self.database.content().is_empty() {
                    self.host.content().to_string()
                } else {
                    format!("{}@{}", self.database.content(), self.host.content())
                }
            }
            DatabaseType::SQLite => self
                .sqlite_path
                .content()
                .file_name_for_display()
                .unwrap_or("SQLite")
                .to_string(),
        }
    }

    pub fn field_value(&self, field: ConnectionField) -> &str {
        match field {
            ConnectionField::DatabaseType => "",
            ConnectionField::Name => self.name.content(),
            ConnectionField::SqlitePath => self.sqlite_path.content(),
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
            ConnectionField::DatabaseType => None,
            ConnectionField::Name => Some(&self.name),
            ConnectionField::SqlitePath => Some(&self.sqlite_path),
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
            ConnectionField::DatabaseType => None,
            ConnectionField::Name => Some(&mut self.name),
            ConnectionField::SqlitePath => Some(&mut self.sqlite_path),
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

    pub fn set_first_run(&mut self, is_first_run: bool) {
        self.is_first_run = is_first_run;
    }

    pub fn has_errors(&self) -> bool {
        !self.validation_errors.is_empty()
    }

    pub fn is_edit_mode(&self) -> bool {
        self.editing_id.is_some()
    }

    pub fn visible_fields(&self) -> &'static [ConnectionField] {
        ConnectionField::fields_for(self.database_type)
    }

    pub fn next_field(&self) -> Option<ConnectionField> {
        next_visible_field(self.visible_fields(), self.focused_field)
    }

    pub fn prev_field(&self) -> Option<ConnectionField> {
        prev_visible_field(self.visible_fields(), self.focused_field)
    }

    pub fn focus_next_field(&mut self) {
        if let Some(next) = self.next_field() {
            self.focused_field = next;
        }
    }

    pub fn focus_prev_field(&mut self) {
        if let Some(prev) = self.prev_field() {
            self.focused_field = prev;
        }
    }

    pub fn set_database_type(&mut self, database_type: DatabaseType) {
        self.database_type = database_type;
        self.database_type_dropdown.is_open = false;
        self.ssl_dropdown.is_open = false;
        if !self.visible_fields().contains(&self.focused_field) {
            self.focused_field = ConnectionField::DatabaseType;
        }
        let visible_fields = self.visible_fields();
        self.validation_errors
            .retain(|field, _| visible_fields.contains(field));
    }

    pub fn toggle_focused_dropdown(&mut self) {
        match self.focused_field {
            ConnectionField::DatabaseType => {
                self.database_type_dropdown.is_open = !self.database_type_dropdown.is_open;
                self.ssl_dropdown.is_open = false;
                if self.database_type_dropdown.is_open {
                    self.database_type_dropdown.selected_index = DatabaseType::all()
                        .iter()
                        .position(|v| *v == self.database_type)
                        .unwrap_or(0);
                }
            }
            ConnectionField::SslMode => {
                self.ssl_dropdown.is_open = !self.ssl_dropdown.is_open;
                self.database_type_dropdown.is_open = false;
                if self.ssl_dropdown.is_open {
                    self.ssl_dropdown.selected_index = SslMode::all_variants()
                        .iter()
                        .position(|v| *v == self.ssl_mode)
                        .unwrap_or(2);
                }
            }
            _ => {}
        }
    }

    pub fn dropdown_next(&mut self) {
        if self.database_type_dropdown.is_open {
            let max = DatabaseType::all().len() - 1;
            if self.database_type_dropdown.selected_index < max {
                self.database_type_dropdown.selected_index += 1;
            }
        } else if self.ssl_dropdown.is_open {
            let max = SslMode::all_variants().len() - 1;
            if self.ssl_dropdown.selected_index < max {
                self.ssl_dropdown.selected_index += 1;
            }
        }
    }

    pub fn dropdown_prev(&mut self) {
        if self.database_type_dropdown.is_open {
            self.database_type_dropdown.selected_index =
                self.database_type_dropdown.selected_index.saturating_sub(1);
        } else if self.ssl_dropdown.is_open {
            self.ssl_dropdown.selected_index = self.ssl_dropdown.selected_index.saturating_sub(1);
        }
    }

    pub fn confirm_dropdown(&mut self) {
        if self.database_type_dropdown.is_open {
            if let Some(database_type) =
                DatabaseType::all().get(self.database_type_dropdown.selected_index)
            {
                self.set_database_type(*database_type);
            }
        } else if self.ssl_dropdown.is_open {
            if let Some(mode) = SslMode::all_variants().get(self.ssl_dropdown.selected_index) {
                self.ssl_mode = *mode;
            }
            self.ssl_dropdown.is_open = false;
        }
    }

    pub fn cancel_dropdown(&mut self) {
        self.database_type_dropdown.is_open = false;
        self.ssl_dropdown.is_open = false;
    }

    pub fn record_sqlite_config_error(&mut self, error: SqliteConnectionConfigError) {
        let message = match error {
            SqliteConnectionConfigError::EmptyPath => "Required",
            SqliteConnectionConfigError::UnsupportedPath => "Unsupported characters",
        };
        self.validation_errors
            .insert(ConnectionField::SqlitePath, message.to_string());
    }

    pub fn to_connection_config(&self) -> Result<ConnectionConfig, SqliteConnectionConfigError> {
        Ok(match self.database_type {
            DatabaseType::PostgreSQL => ConnectionConfig::PostgreSQL(
                crate::domain::connection::PostgresConnectionConfig::new(
                    self.host.content().to_string(),
                    self.port
                        .content()
                        .parse()
                        .expect("port validated before building connection config"),
                    self.database.content().to_string(),
                    self.user.content().to_string(),
                    self.password.content().to_string(),
                    self.ssl_mode,
                ),
            ),
            DatabaseType::SQLite => {
                ConnectionConfig::SQLite(crate::domain::connection::SqliteConnectionConfig::new(
                    self.sqlite_path.content().to_string(),
                )?)
            }
        })
    }
}

fn base_from_profile(profile: &ConnectionProfile) -> ConnectionSetupState {
    let name = profile.name.as_str();
    ConnectionSetupState {
        name: TextInputState::new(name, name.chars().count()),
        is_first_run: false,
        editing_id: Some(profile.id.clone()),
        ..ConnectionSetupState::default()
    }
}

trait PathDisplayName {
    fn file_name_for_display(&self) -> Option<&str>;
}

impl PathDisplayName for str {
    fn file_name_for_display(&self) -> Option<&str> {
        Path::new(self)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
    }
}

fn next_visible_field(
    fields: &[ConnectionField],
    current: ConnectionField,
) -> Option<ConnectionField> {
    let idx = fields.iter().position(|field| *field == current)?;
    fields.get(idx + 1).copied()
}

fn prev_visible_field(
    fields: &[ConnectionField],
    current: ConnectionField,
) -> Option<ConnectionField> {
    let idx = fields.iter().position(|field| *field == current)?;
    idx.checked_sub(1).and_then(|idx| fields.get(idx).copied())
}

impl From<&ConnectionProfile> for ConnectionSetupState {
    fn from(profile: &ConnectionProfile) -> Self {
        let mut state = base_from_profile(profile);
        match &profile.config {
            ConnectionConfig::PostgreSQL(config) => {
                let port_str = config.port.to_string();
                state.database_type = DatabaseType::PostgreSQL;
                state.host = TextInputState::new(&config.host, config.host.chars().count());
                state.port = TextInputState::new(&port_str, port_str.chars().count());
                state.database =
                    TextInputState::new(&config.database, config.database.chars().count());
                state.user = TextInputState::new(&config.username, config.username.chars().count());
                state.password =
                    TextInputState::new(&config.password, config.password.chars().count());
                state.ssl_mode = config.ssl_mode;
            }
            ConnectionConfig::SQLite(config) => {
                state.database_type = DatabaseType::SQLite;
                state.sqlite_path =
                    TextInputState::new(config.path(), config.path().chars().count());
            }
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod connection_field {
        use super::*;

        #[rstest]
        #[case(ConnectionField::DatabaseType, false)]
        #[case(ConnectionField::Name, true)]
        #[case(ConnectionField::SqlitePath, true)]
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
            assert_eq!(all.len(), 9);
            assert_eq!(all[0], ConnectionField::DatabaseType);
            assert_eq!(all[8], ConnectionField::SslMode);
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
            assert_eq!(state.focused_field, ConnectionField::DatabaseType);
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
        fn sqlite_default_name_uses_path_file_name() {
            let mut state = ConnectionSetupState {
                database_type: DatabaseType::SQLite,
                ..ConnectionSetupState::default()
            };
            state.sqlite_path.set_content("/tmp/app.db".to_string());

            assert_eq!(state.default_name(), "app.db");
        }

        #[test]
        fn sqlite_config_build_returns_validation_error() {
            let state = ConnectionSetupState {
                database_type: DatabaseType::SQLite,
                ..ConnectionSetupState::default()
            };

            let result = state.to_connection_config();

            assert!(matches!(
                result,
                Err(SqliteConnectionConfigError::EmptyPath)
            ));
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

            let state = ConnectionSetupState::from(&profile);

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
            let state = ConnectionSetupState::from(&profile);
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
