mod tests {
    use super::super::SqliteAdapter;
    use crate::domain::connection::ConnectionProfile;
    use rstest::rstest;

    async fn path_passed_to_adapter(path: &str) -> String {
        let profile = ConnectionProfile::new_sqlite("Local", path).unwrap();
        let snapshot = SqliteAdapter::new()
            .fetch_core_diagnostics(profile.sqlite_config().unwrap())
            .await
            .unwrap();
        snapshot.db_file.ok_value().unwrap().to_string()
    }

    #[rstest]
    #[case::inner_spaces("/tmp/my dir/app db.sqlite")]
    #[case::surrounding_spaces(" /tmp/app.db ")]
    #[case::non_ascii("/tmp/データ/アプリ.db")]
    #[case::relative("data/app.db")]
    #[tokio::test]
    async fn saved_paths_reach_the_adapter_unchanged(#[case] path: &str) {
        assert_eq!(path_passed_to_adapter(path).await, path);
    }
}
