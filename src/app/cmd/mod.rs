pub(in crate::cmd) mod browse;
pub mod cli_sqlite;
pub mod completion_engine;
pub(in crate::cmd) mod connection;
pub mod effect;
pub(in crate::cmd) mod er;
mod metadata_task;
pub mod render_schedule;
pub mod runner;
pub(in crate::cmd) mod settings;
mod single_task_owner;
pub(in crate::cmd) mod sql_editor;
pub(in crate::cmd) mod sqlite_diagnostics;
pub(in crate::cmd) mod sqlite_path_validate;
#[cfg(test)]
pub(crate) mod test_fixtures;
pub(in crate::cmd) mod utility;
