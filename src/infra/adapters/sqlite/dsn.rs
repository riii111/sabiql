mod tests {
    use crate::domain::connection::ConnectionProfile;
    use rstest::rstest;

    #[rstest]
    #[case::inner_spaces("/tmp/my dir/app db.sqlite")]
    #[case::surrounding_spaces(" /tmp/app.db ")]
    #[case::non_ascii("/tmp/データ/アプリ.db")]
    #[case::relative("data/app.db")]
    fn saved_paths_are_preserved(#[case] path: &str) {
        let profile = ConnectionProfile::new_sqlite("Local", path).unwrap();

        assert_eq!(profile.sqlite_config().unwrap().path(), path);
    }
}
