use crate::domain::{ConnectionId, DatabaseType};
use crate::model::app_state::AppState;

pub(super) fn activate_postgres_connection(state: &mut AppState, dsn: &str) {
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "postgres",
        DatabaseType::PostgreSQL,
        dsn,
    );
}

pub(super) fn activate_sqlite_connection(state: &mut AppState, dsn: &str) {
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "sqlite",
        DatabaseType::SQLite,
        dsn,
    );
}
