use crate::adapters::test_support;
use crate::app::ports::outbound::{AccessMode, SqlDialect};
use crate::domain::{
    CommandTag, DatabaseType, QueryResult, QuerySource, QueryValue, SqlitePathError,
    sqlite_explain_query_plan_text_from_result,
};

use super::*;

#[path = "executor_adhoc_session_tests.rs"]
mod adhoc_session;
#[path = "executor_constraints_tests.rs"]
mod constraints;
#[path = "executor_export_tests.rs"]
mod export;
#[path = "executor_preview_tests.rs"]
mod preview;
#[path = "executor_transaction_refresh_tests.rs"]
mod transaction_refresh;
