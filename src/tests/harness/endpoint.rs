use std::sync::Arc;

use sabiql_domain::DatabaseType;
use sabiql_domain::connection::{
    ConnectionEndpoint, MySqlConnectionConfig, MySqlSslMode, PostgresConnectionConfig,
    PostgresEndpoint, SqliteConnectionConfig, SslMode,
};

pub fn postgres(label: &str) -> Arc<ConnectionEndpoint> {
    Arc::new(ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Settings(
        PostgresConnectionConfig::new(label, 5432, "app", "user", "", SslMode::Prefer),
    )))
}

pub fn service(name: &str) -> Arc<ConnectionEndpoint> {
    Arc::new(ConnectionEndpoint::PostgreSQL(PostgresEndpoint::Service(
        name.to_string(),
    )))
}

pub fn sqlite(path: &str) -> Arc<ConnectionEndpoint> {
    Arc::new(ConnectionEndpoint::SQLite(
        SqliteConnectionConfig::new(path).expect("test SQLite path is valid"),
    ))
}

pub fn mysql(label: &str, database: Option<&str>) -> Arc<ConnectionEndpoint> {
    Arc::new(ConnectionEndpoint::MySQL(MySqlConnectionConfig::new(
        label,
        3306,
        database.map(str::to_string),
        "user",
        "",
        MySqlSslMode::Preferred,
    )))
}

pub fn for_engine(database_type: DatabaseType, label: &str) -> Arc<ConnectionEndpoint> {
    match database_type {
        DatabaseType::PostgreSQL => postgres(label),
        DatabaseType::SQLite => sqlite(label),
        DatabaseType::MySQL => mysql(label, None),
    }
}
