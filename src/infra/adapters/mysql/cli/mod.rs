#![allow(
    clippy::redundant_pub_crate,
    reason = "The private cli module keeps these cross-file helpers crate-internal"
)]

use std::time::Duration;

use crate::domain::QueryValue;

mod args;
mod error;
mod export;
mod process;
mod xml;

pub(crate) const MYSQL_PROBE_TIMEOUT: Duration = Duration::from_secs(11);
pub(crate) const MYSQL_QUERY_TIMEOUT: Duration = Duration::from_secs(31);
pub(crate) const MYSQL_EXPORT_TIMEOUT: Duration =
    Duration::from_secs(MYSQL_QUERY_TIMEOUT.as_secs() * 10);
pub(crate) const MYSQL_PROBE_QUERY: &str = "SELECT JSON_OBJECT('database', DATABASE(), 'user', CURRENT_USER(), 'version', VERSION(), 'sql_mode', @@SESSION.sql_mode)";
pub(crate) const MYSQL_READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";
pub(crate) const MYSQL_SESSION_MARKER_COLUMN: &str = "__sabiql_session_marker";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MysqlResultSet {
    pub(crate) columns: Vec<String>,
    pub(crate) values: Vec<Vec<QueryValue>>,
}

pub(crate) use args::{mysql_metadata_args, mysql_probe_args, mysql_query_args};
pub(crate) use error::{classify_mysql_probe_failure, clean_stderr};
pub(crate) use export::export_mysql_csv_to_file;
#[cfg(all(unix, feature = "test-support"))]
pub(crate) use process::run_mysql_cli_script_for_test;
pub(crate) use process::{
    MysqlMetadataSession, MysqlProcess, mysql_metadata_columns,
    run_mysql_adhoc_with_program_and_statements, run_mysql_command, run_mysql_single_statement,
};
