use std::ffi::OsStr;

use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DbOperationError};

use super::super::xml::{MySqlResultSet, parse_mysql_xml};
use super::{
    MySqlProcess, cleanup_mysql_process, configure_mysql_session, finish_mysql_session,
    finish_mysql_session_after_preview_frame, read_one_mysql_resultset, validate_mode_probe,
    write_mysql_statement,
};

pub(in crate::adapters::mysql) struct MySqlMetadataSession {
    process: MySqlProcess,
}

const MYSQL_PREVIEW_COMPLETION_MARKER_COLUMN: &str = "__sabiql_preview_completion";

impl MySqlMetadataSession {
    pub(in crate::adapters::mysql) fn spawn_with_program(
        program: &OsStr,
        option_file: &std::path::Path,
    ) -> Result<Self, DbOperationError> {
        Ok(Self {
            process: MySqlProcess::spawn_with_program(program, option_file)?,
        })
    }

    pub(in crate::adapters::mysql) async fn probe(&mut self) -> Result<(), DbOperationError> {
        let marker = Uuid::new_v4().simple().to_string();
        let query =
            format!("SELECT '{marker}' AS __sabiql_probe, @@SESSION.sql_mode AS __sabiql_sql_mode");
        let result = self.execute(&query).await?;
        validate_mode_probe(&result, &marker)
    }

    pub(in crate::adapters::mysql) async fn prepare_read_only(
        &mut self,
    ) -> Result<(), DbOperationError> {
        configure_mysql_session(&mut self.process, AccessMode::ReadOnly).await
    }

    pub(in crate::adapters::mysql) async fn execute(
        &mut self,
        query: &str,
    ) -> Result<MySqlResultSet, DbOperationError> {
        self.execute_with_expected_columns(query, &[]).await
    }

    pub(in crate::adapters::mysql) async fn execute_with_expected_columns(
        &mut self,
        query: &str,
        expected_columns: &[&str],
    ) -> Result<MySqlResultSet, DbOperationError> {
        write_mysql_statement(&mut self.process, query).await?;
        let xml = read_one_mysql_resultset(&mut self.process).await?;
        let mut result = parse_mysql_xml(&xml)?;
        if result.columns.is_empty() && result.values.is_empty() {
            result.columns = expected_columns
                .iter()
                .map(|column| (*column).to_string())
                .collect();
        }
        Ok(result)
    }

    pub(in crate::adapters::mysql) async fn finish(&mut self) -> Result<(), DbOperationError> {
        let result = finish_mysql_session(&mut self.process).await?;
        if super::has_mysql_cli_error(&result.error_bytes) {
            return Err(super::classify_mysql_query_failure(&result.error_bytes));
        }
        if !result.status.success() && !result.forcibly_stopped {
            return Err(super::classify_mysql_query_failure(&result.error_bytes));
        }
        Ok(())
    }

    pub(in crate::adapters::mysql) async fn finish_preview(
        &mut self,
    ) -> Result<(), DbOperationError> {
        let marker = Uuid::new_v4().simple().to_string();
        let marker_result = self
            .execute(&format!(
                "SELECT '{marker}' AS {MYSQL_PREVIEW_COMPLETION_MARKER_COLUMN}"
            ))
            .await?;
        if marker_result.columns != [MYSQL_PREVIEW_COMPLETION_MARKER_COLUMN]
            || marker_result.values.len() != 1
            || marker_result.values[0].len() != 1
            || marker_result.values[0][0].as_str() != Some(marker.as_str())
        {
            return Err(DbOperationError::QueryFailed(
                "mysql preview completion marker did not match".to_string(),
            ));
        }
        let result = finish_mysql_session_after_preview_frame(&mut self.process).await?;
        if super::has_mysql_cli_error(&result.error_bytes) {
            return Err(super::classify_mysql_query_failure(&result.error_bytes));
        }
        if !result.status.success() && !result.forcibly_stopped {
            return Err(super::classify_mysql_query_failure(&result.error_bytes));
        }
        Ok(())
    }

    pub(in crate::adapters::mysql) async fn cleanup(&mut self) {
        cleanup_mysql_process(&mut self.process).await;
    }
}
