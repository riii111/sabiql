use std::process::Command;

pub(super) fn make_sqlite_dsn(sql: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.db");
    let status = Command::new("sqlite3")
        .arg(&path)
        .arg(sql)
        .status()
        .unwrap();
    assert!(status.success());
    (dir, format!("sqlite://{}", path.display()))
}
