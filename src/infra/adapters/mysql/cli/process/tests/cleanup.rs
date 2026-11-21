use super::*;
use crate::adapters::mysql::dsn::NormalizedMySqlConfig;
use crate::adapters::mysql::option_file::MySqlOptionFile;
use crate::domain::connection::{MySqlConnectionConfig, MySqlSslMode};

#[tokio::test]
async fn bounds_pty_drain_when_the_slave_stays_open() {
    let (master, _slave) = create_mysql_pty().expect("create test PTY");
    let mut pty = MySqlPty {
        input: TokioFile::from_std(master.try_clone().expect("clone PTY master")),
        output: TokioFile::from_std(master),
        pending: Vec::new(),
        frame_scanner: MySqlResultSetFrameScanner::default(),
    };

    assert!(
        tokio::time::timeout(
            MYSQL_PTY_DRAIN_TIMEOUT + Duration::from_secs(1),
            drain_mysql_pty(&mut pty),
        )
        .await
        .is_ok()
    );
}

#[test]
fn metadata_session_owns_option_file_when_mysql_process_start_fails() {
    let config = MySqlConnectionConfig::new(
        "localhost",
        3306,
        None,
        "user",
        "secret",
        MySqlSslMode::Preferred,
    );
    let target = NormalizedMySqlConfig::new(&config).unwrap();
    let option_file = MySqlOptionFile::create(&target).unwrap();
    let path = option_file.path.clone();
    let result = MySqlMetadataSession::spawn_with_metadata_program(
        OsStr::new("__sabiql_missing_mysql_binary__"),
        option_file,
    );

    assert!(result.is_err());
    assert!(!path.exists());
}
