use crate::app::ports::outbound::DbOperationError;
use crate::domain::sqlite_path_from_dsn;

use super::SqliteAdapter;

impl SqliteAdapter {
    pub(in crate::adapters::sqlite) fn path_from_dsn(dsn: &str) -> Result<&str, DbOperationError> {
        sqlite_path_from_dsn(dsn)
            .ok_or_else(|| DbOperationError::ConnectionFailed(format!("Invalid SQLite DSN: {dsn}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::DbAdapterRegistry;
    use crate::app::ports::outbound::DsnBuilder;
    use crate::domain::connection::{ConnectionEndpoint, ConnectionProfile};
    use rstest::rstest;

    fn path_passed_to_adapter(path: &str) -> String {
        let profile = ConnectionProfile::new_sqlite("Local", path).unwrap();
        let dsn = DbAdapterRegistry::new().build_dsn(&ConnectionEndpoint::from(&profile.config));
        SqliteAdapter::path_from_dsn(&dsn).unwrap().to_string()
    }

    #[test]
    fn extracts_path_from_sqlite_dsn() {
        assert_eq!(
            SqliteAdapter::path_from_dsn("sqlite:///tmp/app.db").unwrap(),
            "/tmp/app.db"
        );
    }

    #[test]
    fn rejects_empty_sqlite_dsn() {
        assert!(matches!(
            SqliteAdapter::path_from_dsn("sqlite://"),
            Err(DbOperationError::ConnectionFailed(_))
        ));
    }

    #[rstest]
    #[case::inner_spaces("/tmp/my dir/app db.sqlite")]
    #[case::surrounding_spaces(" /tmp/app.db ")]
    #[case::non_ascii("/tmp/データ/アプリ.db")]
    #[case::relative("data/app.db")]
    fn saved_paths_reach_the_adapter_unchanged(#[case] path: &str) {
        assert_eq!(path_passed_to_adapter(path), path);
    }
}
