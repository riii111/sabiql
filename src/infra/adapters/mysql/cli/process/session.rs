use std::ffi::OsStr;

use uuid::Uuid;

use crate::app::ports::outbound::{AccessMode, DbOperationError};

use super::super::super::option_file::MySqlOptionFile;
use super::super::args::{mysql_metadata_session_args, mysql_query_args};
use super::super::probe::validate_lower_case_table_names;
use super::super::xml::{MySqlResultSet, parse_mysql_preview_xml, parse_mysql_xml};
use super::{
    MySqlProcess, cleanup_mysql_process, configure_mysql_session, finish_mysql_session,
    finish_mysql_session_after_preview_frame, read_one_mysql_resultset,
    validate_mysql_session_exit, write_mysql_statement,
};

pub(in crate::adapters::mysql) struct MySqlMetadataSession {
    process: MySqlProcess,
    _option_file: MySqlOptionFile,
}

const MYSQL_PREVIEW_COMPLETION_MARKER_COLUMN: &str = "__sabiql_preview_completion";
const MYSQL_INSPECTOR_COMPLETION_MARKER_COLUMN: &str = "__sabiql_inspector_completion";

impl MySqlMetadataSession {
    pub(in crate::adapters::mysql) fn spawn_with_program(
        program: &OsStr,
        option_file: MySqlOptionFile,
    ) -> Result<Self, DbOperationError> {
        let args = mysql_query_args(&option_file.path);
        Self::from_process(
            MySqlProcess::spawn_with_preview_program(program, args),
            option_file,
        )
    }

    pub(in crate::adapters::mysql) fn spawn_with_metadata_program(
        program: &OsStr,
        option_file: MySqlOptionFile,
    ) -> Result<Self, DbOperationError> {
        let args = mysql_metadata_session_args(&option_file.path);
        Self::from_process(MySqlProcess::spawn_with_args(program, args), option_file)
    }

    fn from_process(
        process: Result<MySqlProcess, DbOperationError>,
        option_file: MySqlOptionFile,
    ) -> Result<Self, DbOperationError> {
        process.map(|process| Self {
            process,
            _option_file: option_file,
        })
    }

    pub(in crate::adapters::mysql) async fn prepare_read_only_and_probe(
        &mut self,
    ) -> Result<u8, DbOperationError> {
        configure_mysql_session(&mut self.process, AccessMode::ReadOnly).await?;
        let marker = Uuid::new_v4().simple().to_string();
        let query = format!(
            "SELECT '{marker}' AS __sabiql_probe, @@lower_case_table_names AS __sabiql_lower_case_table_names"
        );
        let result = self.execute(&query).await?;
        validate_metadata_probe(&result, &marker)
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
        let mut result = if self.process.preview_byte_budget {
            parse_mysql_preview_xml(&xml)?
        } else {
            parse_mysql_xml(&xml)?
        };
        if result.columns.is_empty() && result.values.is_empty() {
            result.columns = expected_columns
                .iter()
                .map(|column| (*column).to_string())
                .collect();
        }
        Ok(result)
    }

    pub(in crate::adapters::mysql) async fn execute_show_create_with_completion_marker(
        &mut self,
        query: &str,
        expected_columns: &[&str],
    ) -> Result<MySqlResultSet, DbOperationError> {
        let marker = Uuid::new_v4().simple().to_string();
        let marker_query =
            format!("SELECT '{marker}' AS {MYSQL_INSPECTOR_COMPLETION_MARKER_COLUMN}");

        write_mysql_statement(&mut self.process, query).await?;
        write_mysql_statement(&mut self.process, &marker_query).await?;

        let source_xml = read_one_mysql_resultset(&mut self.process).await?;
        let mut source_result = parse_mysql_xml(&source_xml)?;
        if source_result.columns.is_empty() && source_result.values.is_empty() {
            source_result.columns = expected_columns
                .iter()
                .map(|column| (*column).to_string())
                .collect();
        }
        let marker_xml = read_one_mysql_resultset(&mut self.process).await?;
        let marker_result = parse_mysql_xml(&marker_xml)?;
        if marker_result.columns != [MYSQL_INSPECTOR_COMPLETION_MARKER_COLUMN]
            || marker_result.values.len() != 1
            || marker_result.values[0].len() != 1
            || marker_result.values[0][0].as_str() != Some(marker.as_str())
        {
            return Err(DbOperationError::QueryFailed(
                "mysql inspector completion marker did not match".to_string(),
            ));
        }
        let result = finish_mysql_session_after_preview_frame(&mut self.process).await?;
        validate_mysql_session_exit(&result, self.process.client_packet_limit_bytes)?;
        Ok(source_result)
    }

    pub(in crate::adapters::mysql) async fn finish(&mut self) -> Result<(), DbOperationError> {
        let result = finish_mysql_session(&mut self.process).await?;
        validate_mysql_session_exit(&result, self.process.client_packet_limit_bytes)?;
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
        validate_mysql_session_exit(&result, self.process.client_packet_limit_bytes)?;
        Ok(())
    }

    pub(in crate::adapters::mysql) async fn resolve_timed_result<T>(
        &mut self,
        result: Result<Result<T, DbOperationError>, tokio::time::error::Elapsed>,
    ) -> Result<T, DbOperationError> {
        let result = match result {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(DbOperationError::Timeout(
                "mysql query exceeded the execution timeout".to_string(),
            )),
        };
        cleanup_mysql_process(&mut self.process).await;
        result
    }
}

fn validate_metadata_probe(result: &MySqlResultSet, marker: &str) -> Result<u8, DbOperationError> {
    if result.values.len() != 1
        || result.columns != ["__sabiql_probe", "__sabiql_lower_case_table_names"]
    {
        return Err(DbOperationError::QueryFailed(
            "mysql metadata probe returned an unexpected result".to_string(),
        ));
    }
    let values = &result.values[0];
    if values.len() != 2 || values[0].as_str() != Some(marker) {
        return Err(DbOperationError::QueryFailed(
            "mysql metadata probe returned an unexpected result".to_string(),
        ));
    }
    let lower_case_table_names = values[1]
        .as_str()
        .ok_or_else(|| {
            DbOperationError::QueryFailed(
                "mysql metadata probe returned no lower_case_table_names".to_string(),
            )
        })?
        .parse::<u8>()
        .map_err(|_| {
            DbOperationError::MetadataParseFailed(
                "invalid MySQL lower_case_table_names value".to_string(),
            )
        })?;
    validate_lower_case_table_names(lower_case_table_names)?;
    Ok(lower_case_table_names)
}
