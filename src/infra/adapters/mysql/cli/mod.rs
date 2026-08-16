mod args;
mod error;
mod export;
#[cfg(not(unix))]
mod pipe;
mod policy;
mod probe;
mod process;
#[cfg(unix)]
mod pty;
mod xml;

pub(super) use export::export_mysql_csv_to_file;
pub(super) use policy::{validate_mysql_export_query, validate_mysql_multi_query};
pub(super) use probe::{check_mysql_cli_version, probe_mysql_server};
pub(super) use process::{
    MYSQL_QUERY_TIMEOUT, MySqlMetadataSession, run_mysql_adhoc, run_mysql_single_statement,
};

#[cfg(feature = "test-support")]
pub(super) use process::run_mysql_adhoc_with_timeout_for_test;
pub(super) use xml::MySqlResultSet;

#[cfg(all(unix, feature = "test-support"))]
pub(super) use process::run_mysql_cli_script_for_test;

#[cfg(test)]
pub(super) mod test_support {
    pub(in crate::adapters::mysql) use super::process::MySqlProcess;
}
