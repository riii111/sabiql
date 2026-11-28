mod config;
mod database_type;
mod dsn;
mod endpoint;
mod id;
mod name;
mod profile;
mod service_entry;
mod sqlite_path;
mod ssl_mode;

pub use config::{
    ConnectionConfig, MySqlConnectionConfig, MySqlSslMode, MySqlTransport,
    PostgresConnectionConfig, SqliteConnectionConfig, SqliteConnectionConfigError,
};
pub use database_type::DatabaseType;
pub use dsn::{build_dsn, normalize_mysql_host, postgres_conninfo};
pub use endpoint::{ConnectionEndpoint, PostgresEndpoint};
pub use id::ConnectionId;
pub use name::{ConnectionName, ConnectionNameError};
pub use profile::{ConnectionProfile, ConnectionProfileError};
pub use service_entry::ServiceEntry;
pub use sqlite_path::{
    SqlitePathError, classify_sqlite_metadata_error, classify_sqlite_read_error,
};
pub use ssl_mode::SslMode;
