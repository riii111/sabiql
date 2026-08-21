mod args;
mod diagnostics;
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

use tokio::process::Command;

pub(super) fn sanitize_mysql_command_environment(command: &mut Command) {
    command
        .env_remove("MYSQL_PWD")
        .env_remove("MYSQL_PASSWORD")
        .env_remove("LIBMYSQL_ENABLE_CLEARTEXT_PLUGIN");
}

pub(super) use export::export_mysql_csv_to_file;
pub(super) use policy::{
    validate_mysql_export_query, validate_mysql_multi_query,
    validate_mysql_multi_query_with_lower_case_table_names,
};
pub(super) use probe::{check_mysql_cli_version, probe_mysql_server};
#[cfg(feature = "test-support")]
pub(super) use process::test_support::run_mysql_adhoc_with_timeout_for_test;
pub(super) use process::{
    MYSQL_QUERY_TIMEOUT, MySqlMetadataSession, run_mysql_adhoc, run_mysql_single_statement,
};

pub(super) use xml::MySqlResultSet;

#[cfg(all(unix, feature = "test-support"))]
pub(super) mod test_support;

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn mysql_command_environment_removes_ambient_credentials_and_cleartext_plugin() {
        let mut command = Command::new("mysql");
        command
            .env("MYSQL_PWD", "password")
            .env("MYSQL_PASSWORD", "password")
            .env("LIBMYSQL_ENABLE_CLEARTEXT_PLUGIN", "1")
            .env("SABIQL_TEST_ENVIRONMENT", "preserved");

        sanitize_mysql_command_environment(&mut command);

        for name in [
            "MYSQL_PWD",
            "MYSQL_PASSWORD",
            "LIBMYSQL_ENABLE_CLEARTEXT_PLUGIN",
        ] {
            assert!(
                command
                    .as_std()
                    .get_envs()
                    .any(|(key, value)| key == OsStr::new(name) && value.is_none())
            );
        }
        assert!(command
            .as_std()
            .get_envs()
            .any(|(key, value)| key == OsStr::new("SABIQL_TEST_ENVIRONMENT")
                && value.is_some()));
    }
}
