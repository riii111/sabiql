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

#[cfg(test)]
pub(super) fn assert_connection_save_fetch_effects(
    effects: &[crate::cmd::effect::Effect],
    database_type: DatabaseType,
) {
    use crate::cmd::effect::Effect;

    match database_type {
        DatabaseType::SQLite => {
            assert_eq!(effects.len(), 1, "sqlite save should emit Sequence");
            let Effect::Sequence(seq) = &effects[0] else {
                panic!("expected Sequence, got {effects:?}");
            };
            assert_eq!(seq.len(), 3);
            assert!(matches!(seq[0], Effect::CacheInvalidate { .. }));
            assert!(matches!(seq[1], Effect::ClearCompletionEngineCache));
            assert!(matches!(seq[2], Effect::FetchMetadata { .. }));
        }
        DatabaseType::PostgreSQL => {
            assert_eq!(
                effects.len(),
                2,
                "postgres save should preserve prefetched metadata cache"
            );
            assert!(matches!(effects[0], Effect::ClearCompletionEngineCache));
            assert!(matches!(effects[1], Effect::FetchMetadata { .. }));
        }
    }
}
