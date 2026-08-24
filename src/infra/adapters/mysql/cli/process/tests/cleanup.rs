use super::*;
use crate::adapters::mysql::dsn::parse_mysql_dsn;
use crate::adapters::mysql::option_file::MySqlOptionFile;

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
    let target = parse_mysql_dsn("mysql://user:secret@localhost:3306").unwrap();
    let option_file = MySqlOptionFile::create(&target).unwrap();
    let path = option_file.path.clone();
    let result = MySqlMetadataSession::spawn_with_metadata_program(
        OsStr::new("__sabiql_missing_mysql_binary__"),
        option_file,
    );

    assert!(result.is_err());
    assert!(!path.exists());
}
