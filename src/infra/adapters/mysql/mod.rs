mod adapter;
mod cli;
mod connection;
mod dsn;
mod executor;
mod metadata;
mod option_file;
mod sql;

#[cfg(feature = "test-support")]
mod test_support;

pub use adapter::MySqlAdapter;

#[cfg(all(unix, feature = "test-support"))]
pub use test_support::run_mysql_cli_script_for_test;

#[cfg(feature = "test-support")]
pub use test_support::{
    execute_mysql_adhoc_with_read_only_session_for_test, execute_mysql_adhoc_with_timeout_for_test,
    export_mysql_csv_to_path_for_test, run_mysql_cli_query_for_test,
};
