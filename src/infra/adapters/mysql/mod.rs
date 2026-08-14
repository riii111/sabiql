mod adapter;
mod cli;
mod connection;
mod dsn;
mod executor;
mod metadata;
mod option_file;
mod sql;

pub use adapter::MySqlAdapter;

#[cfg(all(unix, feature = "test-support"))]
pub use executor::test_support::run_mysql_cli_script_for_test;

#[cfg(feature = "test-support")]
pub use executor::test_support::export_mysql_csv_to_path_for_test;
