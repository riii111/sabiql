use std::collections::HashMap;

use crate::domain::connection::{
    ConnectionConfig, ConnectionId, ConnectionProfile, DatabaseType, MySqlConnectionConfig,
    MySqlSslMode, PostgresConnectionConfig, SqliteConnectionConfig, SqliteConnectionConfigError,
    SqlitePathError, SslMode,
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
    SslCa,
    SslCert,
    SslKey,
}

impl ConnectionField {
    pub fn fields_for(
        database_type: DatabaseType,
        mysql_ssl_mode: MySqlSslMode,
    ) -> &'static [Self] {
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
            DatabaseType::MySQL => match mysql_ssl_mode {
                MySqlSslMode::Disabled => &[
                    Self::DatabaseType,
                    Self::Name,
                    Self::Host,
                    Self::Port,
                    Self::Database,
                    Self::User,
                    Self::Password,
                    Self::SslMode,
                ],
                MySqlSslMode::Preferred | MySqlSslMode::Required => &[
                    Self::DatabaseType,
                    Self::Name,
                    Self::Host,
                    Self::Port,
                    Self::Database,
                    Self::User,
                    Self::Password,
                    Self::SslMode,
                    Self::SslCert,
                    Self::SslKey,
                ],
                MySqlSslMode::VerifyCa | MySqlSslMode::VerifyIdentity => &[
                    Self::DatabaseType,
                    Self::Name,
                    Self::Host,
                    Self::Port,
                    Self::Database,
                    Self::User,
                    Self::Password,
                    Self::SslMode,
                    Self::SslCa,
                    Self::SslCert,
                    Self::SslKey,
                ],
            },
        }
    }

    pub fn is_required(self) -> bool {
        matches!(
            self,
            Self::Name | Self::SqlitePath | Self::Port | Self::Database
        )
    }

    pub fn max_chars(self) -> Option<usize> {
        match self {
            Self::Name => Some(50),
            Self::SqlitePath | Self::SslCa | Self::SslCert | Self::SslKey => Some(4096),
            Self::Host | Self::Database | Self::User | Self::Password => Some(255),
            Self::Port => Some(5),
            Self::DatabaseType | Self::SslMode => None,
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Host | Self::User | Self::Password => "empty = psql default",
            _ => "",
        }
    }

    pub fn placeholder_for(self, database_type: DatabaseType) -> &'static str {
        if database_type == DatabaseType::MySQL {
            return match self {
                Self::Password => "empty = no password",
                _ => "",
            };
        }
        self.placeholder()
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
            Self::SslCa => "CA Path:",
            Self::SslCert => "Cert Path:",
            Self::SslKey => "Key Path:",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SslModeDropdown {
    is_open: bool,
    selected_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct DatabaseTypeDropdown {
    is_open: bool,
    selected_index: usize,
}

impl SslModeDropdown {
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }
}

impl DatabaseTypeDropdown {
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionSetupState {
    pub(crate) database_type: DatabaseType,
    pub(crate) name: TextInputState,
    pub(crate) sqlite_path: TextInputState,
    pub(crate) host: TextInputState,
    pub(crate) port: TextInputState,
    pub(crate) database: TextInputState,
    pub(crate) user: TextInputState,
    pub(crate) password: TextInputState,
    pub(crate) ssl_ca: TextInputState,
    pub(crate) ssl_cert: TextInputState,
    pub(crate) ssl_key: TextInputState,
    pub(crate) ssl_mode: SslMode,
    pub(crate) mysql_ssl_mode: MySqlSslMode,

    pub(crate) focused_field: ConnectionField,
    pub(crate) database_type_dropdown: DatabaseTypeDropdown,
    pub(crate) ssl_dropdown: SslModeDropdown,
    pub(crate) validation_errors: HashMap<ConnectionField, String>,

    is_first_run: bool,

    pub(crate) editing_id: Option<ConnectionId>,
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
            ssl_ca: TextInputState::default(),
            ssl_cert: TextInputState::default(),
            ssl_key: TextInputState::default(),
            ssl_mode: SslMode::Prefer,
            mysql_ssl_mode: MySqlSslMode::Preferred,
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
    pub fn database_type(&self) -> DatabaseType {
        self.database_type
    }

    pub fn ssl_mode(&self) -> SslMode {
        self.ssl_mode
    }

    pub fn mysql_ssl_mode(&self) -> MySqlSslMode {
        self.mysql_ssl_mode
    }

    pub fn focused_field(&self) -> ConnectionField {
        self.focused_field
    }

    pub fn is_first_run(&self) -> bool {
        self.is_first_run
    }

    pub fn editing_id(&self) -> Option<&ConnectionId> {
        self.editing_id.as_ref()
    }

    pub fn database_type_dropdown(&self) -> &DatabaseTypeDropdown {
        &self.database_type_dropdown
    }

    pub fn ssl_dropdown(&self) -> &SslModeDropdown {
        &self.ssl_dropdown
    }

    pub fn validation_error(&self, field: ConnectionField) -> Option<&str> {
        self.validation_errors.get(&field).map(String::as_str)
    }

    pub fn has_validation_error(&self, field: ConnectionField) -> bool {
        self.validation_errors.contains_key(&field)
    }

    pub fn has_validation_errors(&self) -> bool {
        !self.validation_errors.is_empty()
    }

    pub fn clear_validation_error(&mut self, field: ConnectionField) {
        self.validation_errors.remove(&field);
    }

    pub fn set_validation_error(&mut self, field: ConnectionField, message: impl Into<String>) {
        self.validation_errors.insert(field, message.into());
    }

    pub fn retain_validation_errors_for_visible_fields(&mut self) {
        let visible_fields = self.visible_fields();
        self.validation_errors
            .retain(|field, _| visible_fields.contains(field));
    }

    pub fn input(&self, field: ConnectionField) -> Option<&TextInputState> {
        match field {
            ConnectionField::DatabaseType | ConnectionField::SslMode => None,
            ConnectionField::Name => Some(&self.name),
            ConnectionField::SqlitePath => Some(&self.sqlite_path),
            ConnectionField::Host => Some(&self.host),
            ConnectionField::Port => Some(&self.port),
            ConnectionField::Database => Some(&self.database),
            ConnectionField::User => Some(&self.user),
            ConnectionField::Password => Some(&self.password),
            ConnectionField::SslCa => Some(&self.ssl_ca),
            ConnectionField::SslCert => Some(&self.ssl_cert),
            ConnectionField::SslKey => Some(&self.ssl_key),
        }
    }

    pub fn field_value(&self, field: ConnectionField) -> &str {
        self.input(field).map_or("", TextInputState::content)
    }

    pub fn input_mut(&mut self, field: ConnectionField) -> Option<&mut TextInputState> {
        match field {
            ConnectionField::DatabaseType | ConnectionField::SslMode => None,
            ConnectionField::Name => Some(&mut self.name),
            ConnectionField::SqlitePath => Some(&mut self.sqlite_path),
            ConnectionField::Host => Some(&mut self.host),
            ConnectionField::Port => Some(&mut self.port),
            ConnectionField::Database => Some(&mut self.database),
            ConnectionField::User => Some(&mut self.user),
            ConnectionField::Password => Some(&mut self.password),
            ConnectionField::SslCa => Some(&mut self.ssl_ca),
            ConnectionField::SslCert => Some(&mut self.ssl_cert),
            ConnectionField::SslKey => Some(&mut self.ssl_key),
        }
    }

    pub fn port_mut(&mut self) -> &mut TextInputState {
        &mut self.port
    }

    pub fn focused_input(&self) -> Option<&TextInputState> {
        self.input(self.focused_field)
    }

    pub fn focused_input_mut(&mut self) -> Option<&mut TextInputState> {
        self.input_mut(self.focused_field)
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

    pub fn is_edit_mode(&self) -> bool {
        self.editing_id.is_some()
    }

    pub fn visible_fields(&self) -> &'static [ConnectionField] {
        ConnectionField::fields_for(self.database_type, self.mysql_ssl_mode)
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
        let previous = self.database_type;
        self.database_type = database_type;
        if previous != database_type {
            let default_port = match database_type {
                DatabaseType::PostgreSQL | DatabaseType::SQLite => "5432",
                DatabaseType::MySQL => "3306",
            };
            if matches!(self.port.content(), "5432" | "3306") {
                self.port.set_content(default_port.to_string());
            }
        }
        self.database_type_dropdown.is_open = false;
        self.ssl_dropdown.is_open = false;
        if !self.visible_fields().contains(&self.focused_field) {
            self.focused_field = ConnectionField::DatabaseType;
        }
        self.retain_validation_errors_for_visible_fields();
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
                    self.ssl_dropdown.selected_index = if self.database_type == DatabaseType::MySQL
                    {
                        MySqlSslMode::all_variants()
                            .iter()
                            .position(|v| *v == self.mysql_ssl_mode)
                            .unwrap_or(1)
                    } else {
                        SslMode::all_variants()
                            .iter()
                            .position(|v| *v == self.ssl_mode)
                            .unwrap_or(2)
                    };
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
            let max = if self.database_type == DatabaseType::MySQL {
                MySqlSslMode::all_variants().len() - 1
            } else {
                SslMode::all_variants().len() - 1
            };
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
            if self.database_type == DatabaseType::MySQL {
                if let Some(mode) =
                    MySqlSslMode::all_variants().get(self.ssl_dropdown.selected_index)
                {
                    self.mysql_ssl_mode = *mode;
                    self.retain_validation_errors_for_visible_fields();
                }
            } else if let Some(mode) = SslMode::all_variants().get(self.ssl_dropdown.selected_index)
            {
                self.ssl_mode = *mode;
            }
            self.ssl_dropdown.is_open = false;
        }
    }

    pub fn cancel_dropdown(&mut self) {
        self.database_type_dropdown.is_open = false;
        self.ssl_dropdown.is_open = false;
    }

    pub fn has_open_dropdown(&self) -> bool {
        self.database_type_dropdown.is_open || self.ssl_dropdown.is_open
    }

    pub fn record_sqlite_config_error(&mut self, error: SqliteConnectionConfigError) {
        let message = match error {
            SqliteConnectionConfigError::EmptyPath => "Required",
            SqliteConnectionConfigError::UnsupportedPath => "Unsupported characters",
            SqliteConnectionConfigError::UnsupportedInMemoryDatabase => {
                "In-memory SQLite databases cannot retain contents because sabiql starts sqlite3 per operation; use a temporary file"
            }
            SqliteConnectionConfigError::UnsupportedUriFilename => {
                "SQLite URI filenames are not supported; use a regular file path"
            }
        };
        self.validation_errors
            .insert(ConnectionField::SqlitePath, message.to_string());
    }

    pub fn record_sqlite_path_error(&mut self, error: SqlitePathError) {
        let message = match error {
            SqlitePathError::FileNotFound(_) => "File not found",
            SqlitePathError::IsDirectory(_) => "Path is a directory",
            SqlitePathError::NotRegularFile(_) => "Not a regular file",
            SqlitePathError::NotDatabaseFile(_) => "Not a SQLite database",
            SqlitePathError::ReadAccessDenied(_) => "Read permission denied",
            SqlitePathError::PathAccessDenied(_) => "Access denied",
            SqlitePathError::Io(_) => "Cannot access file",
        };
        self.validation_errors
            .insert(ConnectionField::SqlitePath, message.to_string());
    }

    pub fn to_connection_config(&self) -> Result<ConnectionConfig, SqliteConnectionConfigError> {
        Ok(match self.database_type {
            DatabaseType::PostgreSQL => {
                ConnectionConfig::PostgreSQL(PostgresConnectionConfig::new(
                    self.host.content().trim().to_string(),
                    self.port
                        .content()
                        .parse()
                        .expect("port validated before building connection config"),
                    self.database.content().trim().to_string(),
                    self.user.content().trim().to_string(),
                    self.password.content().to_string(),
                    self.ssl_mode,
                ))
            }
            DatabaseType::SQLite => ConnectionConfig::SQLite(SqliteConnectionConfig::new(
                self.sqlite_path.content().to_string(),
            )?),
            DatabaseType::MySQL => ConnectionConfig::MySQL(
                MySqlConnectionConfig::new(
                    self.host.content().trim().to_string(),
                    self.port
                        .content()
                        .parse()
                        .expect("port validated before building connection config"),
                    match self.database.content().trim() {
                        "" => None,
                        database => Some(database.to_string()),
                    },
                    self.user.content().trim().to_string(),
                    self.password.content().to_string(),
                    self.mysql_ssl_mode,
                )
                .with_tls_paths(
                    (!matches!(self.mysql_ssl_mode, MySqlSslMode::Disabled))
                        .then(|| optional_path(&self.ssl_ca))
                        .flatten(),
                    (!matches!(self.mysql_ssl_mode, MySqlSslMode::Disabled))
                        .then(|| optional_path(&self.ssl_cert))
                        .flatten(),
                    (!matches!(self.mysql_ssl_mode, MySqlSslMode::Disabled))
                        .then(|| optional_path(&self.ssl_key))
                        .flatten(),
                ),
            ),
        })
    }
}

// UI form baseline: inherit ConnectionSetupState::default() and override profile identity only.
fn base_from_profile(profile: &ConnectionProfile) -> ConnectionSetupState {
    let name = profile.name.as_str();
    ConnectionSetupState {
        name: TextInputState::new(name, name.chars().count()),
        is_first_run: false,
        editing_id: Some(profile.id.clone()),
        ..ConnectionSetupState::default()
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
            ConnectionConfig::MySQL(config) => {
                state.database_type = DatabaseType::MySQL;
                state.host = TextInputState::new(&config.host, config.host.chars().count());
                let port_str = config.port.to_string();
                state.port = TextInputState::new(&port_str, port_str.chars().count());
                let database = config.database.as_deref().unwrap_or_default();
                state.database = TextInputState::new(database, database.chars().count());
                state.user = TextInputState::new(&config.username, config.username.chars().count());
                state.password =
                    TextInputState::new(&config.password, config.password.chars().count());
                state.mysql_ssl_mode = config.ssl_mode;
                if let Some(path) = config.ssl_ca.as_deref() {
                    state.ssl_ca = TextInputState::new(path, path.chars().count());
                }
                if let Some(path) = config.ssl_cert.as_deref() {
                    state.ssl_cert = TextInputState::new(path, path.chars().count());
                }
                if let Some(path) = config.ssl_key.as_deref() {
                    state.ssl_key = TextInputState::new(path, path.chars().count());
                }
            }
        }
        state
    }
}

fn optional_path(input: &TextInputState) -> Option<String> {
    (!input.content().trim().is_empty()).then(|| input.content().to_string())
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
        #[case(ConnectionField::Host, false)]
        #[case(ConnectionField::Port, true)]
        #[case(ConnectionField::Database, true)]
        #[case(ConnectionField::User, false)]
        #[case(ConnectionField::Password, false)]
        #[case(ConnectionField::SslMode, false)]
        #[case(ConnectionField::SslCa, false)]
        #[case(ConnectionField::SslCert, false)]
        #[case(ConnectionField::SslKey, false)]
        fn is_required_returns_correct_value(
            #[case] field: ConnectionField,
            #[case] expected: bool,
        ) {
            assert_eq!(field.is_required(), expected);
        }
        #[test]
        fn fields_for_returns_postgres_fields_in_order() {
            let fields =
                ConnectionField::fields_for(DatabaseType::PostgreSQL, MySqlSslMode::Preferred);
            assert_eq!(fields.len(), 8);
            assert_eq!(fields[0], ConnectionField::DatabaseType);
            assert_eq!(fields[7], ConnectionField::SslMode);
        }

        #[test]
        fn fields_for_returns_sqlite_fields_in_order() {
            assert_eq!(
                ConnectionField::fields_for(DatabaseType::SQLite, MySqlSslMode::Preferred),
                &[
                    ConnectionField::DatabaseType,
                    ConnectionField::Name,
                    ConnectionField::SqlitePath
                ]
            );
        }

        #[test]
        fn fields_for_returns_mysql_fields_in_order() {
            assert_eq!(
                ConnectionField::fields_for(DatabaseType::MySQL, MySqlSslMode::Preferred),
                &[
                    ConnectionField::DatabaseType,
                    ConnectionField::Name,
                    ConnectionField::Host,
                    ConnectionField::Port,
                    ConnectionField::Database,
                    ConnectionField::User,
                    ConnectionField::Password,
                    ConnectionField::SslMode,
                    ConnectionField::SslCert,
                    ConnectionField::SslKey,
                ]
            );
        }

        #[test]
        fn max_chars_limits_match_field_policy() {
            assert_eq!(ConnectionField::Name.max_chars(), Some(50));
            assert_eq!(ConnectionField::SqlitePath.max_chars(), Some(4096));
            assert_eq!(ConnectionField::Host.max_chars(), Some(255));
            assert_eq!(ConnectionField::Port.max_chars(), Some(5));
            assert_eq!(ConnectionField::Database.max_chars(), Some(255));
            assert_eq!(ConnectionField::User.max_chars(), Some(255));
            assert_eq!(ConnectionField::Password.max_chars(), Some(255));
            assert_eq!(ConnectionField::DatabaseType.max_chars(), None);
            assert_eq!(ConnectionField::SslMode.max_chars(), None);
        }

        #[rstest]
        #[case(ConnectionField::Host)]
        #[case(ConnectionField::User)]
        #[case(ConnectionField::Password)]
        fn placeholder_uses_psql_default_for_delegated_optional_fields(
            #[case] field: ConnectionField,
        ) {
            assert_eq!(field.placeholder(), "empty = psql default");
        }

        #[test]
        fn mysql_placeholders_do_not_use_postgres_defaults() {
            assert_eq!(
                ConnectionField::Password.placeholder_for(DatabaseType::MySQL),
                "empty = no password"
            );
            assert_eq!(
                ConnectionField::Host.placeholder_for(DatabaseType::MySQL),
                ""
            );
            assert_eq!(
                ConnectionField::User.placeholder_for(DatabaseType::MySQL),
                ""
            );
        }

        #[test]
        fn mysql_tls_fields_follow_selected_mode() {
            let mut state = ConnectionSetupState::default();
            state.set_database_type(DatabaseType::MySQL);
            assert!(!state.visible_fields().contains(&ConnectionField::SslCa));
            assert!(state.visible_fields().contains(&ConnectionField::SslCert));

            state.mysql_ssl_mode = MySqlSslMode::VerifyIdentity;
            assert!(state.visible_fields().contains(&ConnectionField::SslCa));

            state.mysql_ssl_mode = MySqlSslMode::Disabled;
            assert!(!state.visible_fields().contains(&ConnectionField::SslCert));
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
            assert!(state.is_first_run());
            assert!(state.editing_id.is_none());
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
        fn mysql_config_builds_without_initial_database() {
            let mut state = ConnectionSetupState::default();
            state.set_database_type(DatabaseType::MySQL);
            state.name.set_content("MySQL".to_string());
            state.user.set_content("user".to_string());

            let config = state.to_connection_config().unwrap();

            assert!(matches!(
                config,
                ConnectionConfig::MySQL(MySqlConnectionConfig {
                    database: None,
                    port: 3306,
                    ssl_mode: MySqlSslMode::Preferred,
                    ..
                })
            ));
        }

        #[test]
        fn has_errors_returns_false_when_empty() {
            let state = ConnectionSetupState::default();
            assert!(!state.has_validation_errors());
        }

        #[test]
        fn has_errors_returns_true_when_errors_exist() {
            let mut state = ConnectionSetupState::default();
            state
                .validation_errors
                .insert(ConnectionField::Host, "Required".to_string());
            assert!(state.has_validation_errors());
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
            assert!(!state.has_validation_errors());
        }

        #[test]
        fn from_sqlite_profile_inherits_default_field_baselines() {
            let profile = ConnectionProfile::new_sqlite("Local", "/tmp/app.db").unwrap();

            let state = ConnectionSetupState::from(&profile);

            assert_eq!(state.database_type, DatabaseType::SQLite);
            assert_eq!(state.sqlite_path.content(), "/tmp/app.db");
            assert_eq!(state.host.content(), "localhost");
            assert_eq!(state.port.content(), "5432");
            assert_eq!(state.ssl_mode, SslMode::Prefer);
            assert_eq!(state.editing_id, Some(profile.id));
            assert!(!state.is_first_run());
        }

        #[test]
        fn from_profile_populates_all_fields() {
            let profile = ConnectionProfile::new_postgres(
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
            assert!(!state.is_first_run());
        }

        #[test]
        fn is_edit_mode_returns_false_for_new() {
            let state = ConnectionSetupState::default();
            assert!(!state.is_edit_mode());
        }

        #[test]
        fn is_edit_mode_returns_true_for_edit() {
            let profile = ConnectionProfile::new_postgres(
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
