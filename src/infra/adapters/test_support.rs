use std::process::Command;

use sabiql_domain::QueryResult;
use sabiql_domain::connection::SqliteConnectionConfig;

#[must_use]
pub(super) fn display_row(result: &QueryResult, row: usize) -> Vec<String> {
    result
        .display_row_at(row)
        .expect("test result should contain the requested row")
}

#[must_use]
pub(super) fn make_sqlite_db(sql: &str) -> (tempfile::TempDir, SqliteConnectionConfig) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.db");
    let output = Command::new("sqlite3")
        .arg(&path)
        .arg(sql)
        .output()
        .unwrap_or_else(|err| {
            panic!(
                "failed to run sqlite3 fixture setup for {}: {err}",
                path.display()
            )
        });
    assert!(
        output.status.success(),
        "sqlite3 fixture setup failed\npath: {}\nsql:\n{}\nstderr:\n{}",
        path.display(),
        sqlite_setup_context(sql),
        String::from_utf8_lossy(&output.stderr)
    );
    (
        dir,
        SqliteConnectionConfig::new(path.to_str().unwrap()).unwrap(),
    )
}

fn sqlite_setup_context(sql: &str) -> String {
    sql.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join("\n")
}
