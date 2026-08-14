use std::ffi::OsStr;
#[cfg(test)]
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};

use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesRef, Event};
use serde::Deserialize;
#[cfg(unix)]
use tokio::fs::File as TokioFile;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, ReadBuf};
use tokio::process::Child;
use tokio::process::Command;
#[cfg(not(unix))]
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use tokio::time::timeout;
use uuid::Uuid;

use crate::adapters::csv_export::CsvFileWriter;
#[cfg(all(test, unix))]
use crate::adapters::csv_export::export_to_path;
#[cfg(all(test, unix))]
use crate::app::policy::sql::mysql_statement::split_mysql_statements;
use crate::app::policy::sql::mysql_statement::{
    MysqlStatement, MysqlStatementKind, classify_mysql_statement, has_mysql_read_only_side_effect,
};
use crate::app::policy::write::sql_risk::{
    MultiStatementDecision, evaluate_mysql_multi_statement, mysql_statement_is_data_modifying,
    mysql_statement_is_schema_modifying,
};
use crate::app::ports::outbound::{
    AccessMode, DatabaseCli, DbOperationError, MYSQL_SERVER_VERSION_REQUIRED_MARKER,
    MYSQL_SQL_MODE_UNSUPPORTED_MARKER,
};
use crate::domain::{CommandTag, QueryValue, RefreshScope};

#[cfg(all(unix, feature = "test-support"))]
use super::dsn::{parse_mysql_dsn, validate_mysql_tls_files, validate_mysql_values};
use super::{dsn::MySqlDsn, option_file::MySqlOptionFile, sql};

pub(super) const MYSQL_PROBE_TIMEOUT: Duration = Duration::from_secs(11);
pub(super) const MYSQL_QUERY_TIMEOUT: Duration = Duration::from_secs(31);
pub(super) const MYSQL_EXPORT_TIMEOUT: Duration =
    Duration::from_secs(MYSQL_QUERY_TIMEOUT.as_secs() * 10);
pub(super) const MYSQL_PROBE_QUERY: &str = "SELECT JSON_OBJECT('database', DATABASE(), 'user', CURRENT_USER(), 'version', VERSION(), 'sql_mode', @@SESSION.sql_mode)";
pub(super) const MYSQL_READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";
pub(super) const MYSQL_SESSION_MARKER_COLUMN: &str = "__sabiql_session_marker";

include!("probe.rs");
include!("args.rs");
include!("policy.rs");
include!("xml.rs");
include!("pty.rs");
include!("pipe.rs");
include!("process.rs");
include!("error.rs");
include!("export.rs");
include!("process_tests.rs");
include!("xml_tests.rs");
include!("export_tests.rs");
include!("policy_tests.rs");
include!("args_tests.rs");
include!("error_tests.rs");
