use std::process::Command;

use sabiql_domain::{Table, TableKindInfo};

#[must_use]
pub(super) fn minimal_table(schema: impl Into<String>, name: impl Into<String>) -> Table {
    Table {
        schema: schema.into(),
        name: name.into(),
        owner: None,
        columns: Vec::new(),
        primary_key: None,
        foreign_keys: Vec::new(),
        indexes: Vec::new(),
        rls: None,
        triggers: Vec::new(),
        row_count_estimate: None,
        comment: None,
        source_ddl: None,
        kind_info: TableKindInfo::default(),
    }
}

#[must_use]
pub(super) fn make_sqlite_db(sql: &str) -> (tempfile::TempDir, String) {
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
    (dir, format!("sqlite://{}", path.display()))
}

fn sqlite_setup_context(sql: &str) -> String {
    sql.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join("\n")
}
