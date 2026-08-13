use async_trait::async_trait;

use crate::app::ports::outbound::{AccessMode, DbOperationError, MetadataProvider};
use crate::domain::{
    Column, ColumnAttributes, DatabaseMetadata, FkAction, ForeignKey, Index, IndexAttributes,
    IndexType, QueryValue, Schema, Table, TableKind, TableKindInfo, TableSignature, TableSummary,
    Trigger, TriggerEvent, TriggerTiming,
};

use super::{
    MySqlAdapter, MySqlOptionFile, MysqlResultSet, parse_mysql_dsn, run_mysql_adhoc,
    validate_mysql_values,
};

const TABLES_QUERY: &str = "SELECT TABLE_NAME, TABLE_TYPE, TABLE_ROWS, TABLE_COMMENT FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW') UNION ALL SELECT NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW')) ORDER BY TABLE_NAME";
const PREVIEW_IDENTITY_ALIAS_PREFIX: &str = "__sabiql_row_identity_";

#[derive(Debug, Clone)]
struct MysqlColumnMetadata {
    name: String,
    data_type: String,
    nullable: bool,
    default: Option<String>,
    comment: Option<String>,
    ordinal_position: i32,
    primary_key_position: Option<i32>,
    invisible: bool,
    generated: bool,
}

#[derive(Debug, Clone)]
struct MysqlIndexMetadata {
    name: String,
    non_unique: bool,
    index_type: String,
    ordinal_position: i32,
    column_name: String,
    primary: bool,
}

#[derive(Debug, Clone)]
struct MysqlForeignKeyMetadata {
    name: String,
    from_schema: String,
    from_table: String,
    from_column: String,
    to_schema: String,
    to_table: String,
    to_column: String,
    ordinal_position: i32,
    on_update: FkAction,
    on_delete: FkAction,
}

#[derive(Debug, Clone)]
pub(super) struct PreviewMetadata {
    pub visible_columns: Vec<Column>,
    pub order_columns: Vec<String>,
    pub identity_columns: Vec<Column>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConvertedPreviewValues {
    pub visible: Vec<Vec<QueryValue>>,
    pub identity: Option<Vec<Vec<QueryValue>>>,
}

#[derive(Debug, Clone)]
struct MysqlTableMetadata {
    name: String,
    kind: TableKind,
    row_count_estimate: Option<i64>,
    comment: Option<String>,
}

#[derive(Debug, Clone)]
struct MysqlTriggerMetadata {
    name: String,
    timing: TriggerTiming,
    event: TriggerEvent,
    definition: String,
    security_context: Option<String>,
}

#[async_trait]
impl MetadataProvider for MySqlAdapter {
    async fn fetch_metadata(&self, dsn: &str) -> Result<DatabaseMetadata, DbOperationError> {
        let database = selected_database(dsn)?;
        let result = execute_metadata_query(dsn, TABLES_QUERY).await?;
        let tables = parse_table_metadata(&result)?;
        let mut metadata = DatabaseMetadata::new(database.clone());
        metadata.schemas = vec![Schema::new(database.clone())];
        metadata.table_summaries = tables
            .into_iter()
            .map(|table| table_summary(&database, table))
            .collect();
        Ok(metadata)
    }

    async fn fetch_table_detail(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        let metadata = self.fetch_metadata(dsn).await?;
        fetch_table(dsn, schema, table, &metadata.table_summaries).await
    }

    async fn fetch_table_columns_and_fks(
        &self,
        dsn: &str,
        schema: &str,
        table: &str,
    ) -> Result<Table, DbOperationError> {
        let metadata = self.fetch_metadata(dsn).await?;
        fetch_table_columns_and_fks_with_summaries(dsn, schema, table, &metadata.table_summaries)
            .await
    }

    async fn fetch_table_signatures(
        &self,
        dsn: &str,
    ) -> Result<Vec<TableSignature>, DbOperationError> {
        let metadata = self.fetch_metadata(dsn).await?;
        let mut signatures = Vec::with_capacity(metadata.table_summaries.len());
        let summaries = metadata.table_summaries;
        for summary in &summaries {
            let detail = fetch_table_columns_and_fks_with_summaries(
                dsn,
                &summary.schema,
                &summary.name,
                &summaries,
            )
            .await?;
            signatures.push(TableSignature {
                schema: summary.schema.clone(),
                name: summary.name.clone(),
                signature: table_signature(&detail),
            });
        }
        Ok(signatures)
    }
}

pub(super) async fn fetch_preview_metadata(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<PreviewMetadata, DbOperationError> {
    find_table(dsn, schema, table).await?;
    let column_metadata = fetch_columns(dsn, schema, table).await?;
    let columns = column_metadata
        .iter()
        .map(column_from_metadata)
        .collect::<Vec<_>>();
    let visible_columns = columns
        .iter()
        .filter(|column| !column.is_hidden())
        .cloned()
        .collect::<Vec<_>>();
    if visible_columns.is_empty() {
        return Err(DbOperationError::MetadataParseFailed(format!(
            "MySQL object has no visible columns: {schema}.{table}"
        )));
    }
    let primary_key_names = primary_key_names(&column_metadata);
    let identity_columns = if primary_key_names.iter().any(|name| {
        columns
            .iter()
            .find(|column| &column.name == name)
            .is_some_and(Column::is_hidden)
    }) {
        primary_key_names
            .iter()
            .filter_map(|name| columns.iter().find(|column| &column.name == name))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let order_columns = primary_key_names;
    let order_columns = if order_columns.is_empty() {
        visible_columns
            .iter()
            .map(|column| column.name.clone())
            .collect()
    } else {
        order_columns
    };
    Ok(PreviewMetadata {
        visible_columns,
        order_columns,
        identity_columns,
    })
}

pub(super) fn build_preview_query(
    schema: &str,
    table: &str,
    order_columns: &[String],
    visible_columns: &[Column],
    identity_columns: &[Column],
    limit: usize,
    offset: usize,
) -> String {
    let visible_select = visible_columns
        .iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let identity_select = identity_columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            format!(
                "{} AS {}",
                quote_identifier(&column.name),
                quote_identifier(&preview_identity_alias(index)),
            )
        })
        .collect::<Vec<_>>();
    let columns = std::iter::once(visible_select)
        .chain(identity_select)
        .filter(|select| !select.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let order_by = order_columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT {columns} FROM {}.{} ORDER BY {order_by} LIMIT {limit} OFFSET {offset}",
        quote_identifier(schema),
        quote_identifier(table),
    )
}

pub(super) fn convert_preview_values(
    result: &MysqlResultSet,
    columns: &[Column],
    identity_columns: &[Column],
) -> Result<ConvertedPreviewValues, DbOperationError> {
    let expected_columns = columns
        .iter()
        .map(|column| column.name.clone())
        .chain(
            identity_columns
                .iter()
                .enumerate()
                .map(|(index, _)| preview_identity_alias(index)),
        )
        .collect::<Vec<_>>();
    if result.values.is_empty() {
        if result.columns.is_empty() || result.columns == expected_columns {
            return Ok(ConvertedPreviewValues {
                visible: Vec::new(),
                identity: (!identity_columns.is_empty()).then(Vec::new),
            });
        }
        return Err(DbOperationError::MetadataParseFailed(
            "MySQL preview returned an unexpected column count".to_string(),
        ));
    }
    if result.columns != expected_columns {
        return Err(DbOperationError::MetadataParseFailed(
            "MySQL preview returned unexpected columns".to_string(),
        ));
    }
    let rows = result
        .values
        .iter()
        .map(|row| {
            if row.len() != expected_columns.len() {
                return Err(DbOperationError::MetadataParseFailed(
                    "MySQL preview returned an unexpected row width".to_string(),
                ));
            }
            row.iter()
                .zip(columns.iter().chain(identity_columns))
                .map(|(value, column)| Ok(convert_preview_value(value, &column.data_type)))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (visible, identity) = rows.into_iter().fold(
        (Vec::new(), (!identity_columns.is_empty()).then(Vec::new)),
        |(mut visible, mut identity), row| {
            let (visible_row, identity_row) = row.split_at(columns.len());
            visible.push(visible_row.to_vec());
            if let Some(identity) = identity.as_mut() {
                identity.push(identity_row.to_vec());
            }
            (visible, identity)
        },
    );
    Ok(ConvertedPreviewValues { visible, identity })
}

fn preview_identity_alias(index: usize) -> String {
    format!("{PREVIEW_IDENTITY_ALIAS_PREFIX}{index}")
}

fn convert_preview_value(value: &QueryValue, data_type: &str) -> QueryValue {
    let QueryValue::Text(value) = value else {
        return value.clone();
    };
    if is_binary_type(data_type)
        && let Some(bytes) = decode_hex(value)
    {
        return QueryValue::Blob(bytes);
    }
    if is_numeric_type(data_type) && is_sql_numeric_literal(value) {
        return QueryValue::SqlLiteral(value.clone());
    }
    QueryValue::Text(value.clone())
}

async fn fetch_table(
    dsn: &str,
    schema: &str,
    table: &str,
    summaries: &[TableSummary],
) -> Result<Table, DbOperationError> {
    let table_metadata = find_table(dsn, schema, table).await?;
    let columns = fetch_columns(dsn, schema, table).await?;
    let indexes = fetch_indexes(dsn, schema, table).await?;
    let foreign_keys = fetch_foreign_keys(dsn, schema, table, summaries).await?;
    let triggers = fetch_triggers(dsn, schema, table).await?;
    let source_ddl = fetch_source_ddl(dsn, table, table_metadata.kind).await?;
    let primary_key = primary_key_names(&columns);
    let columns = columns.iter().map(column_from_metadata).collect::<Vec<_>>();
    Ok(Table {
        schema: schema.to_string(),
        name: table_metadata.name,
        owner: None,
        columns,
        primary_key: (!primary_key.is_empty()).then_some(primary_key),
        foreign_keys,
        indexes,
        rls: None,
        triggers,
        row_count_estimate: table_metadata.row_count_estimate,
        comment: table_metadata.comment,
        source_ddl: Some(source_ddl),
        kind_info: TableKindInfo {
            kind: table_metadata.kind,
            ..TableKindInfo::default()
        },
    })
}

async fn fetch_table_columns_and_fks_with_summaries(
    dsn: &str,
    schema: &str,
    table: &str,
    summaries: &[TableSummary],
) -> Result<Table, DbOperationError> {
    let table_metadata = find_table(dsn, schema, table).await?;
    let columns = fetch_columns(dsn, schema, table).await?;
    let foreign_keys = fetch_foreign_keys(dsn, schema, table, summaries).await?;
    let primary_key = primary_key_names(&columns);
    Ok(Table {
        schema: schema.to_string(),
        name: table_metadata.name,
        owner: None,
        columns: columns.iter().map(column_from_metadata).collect(),
        primary_key: (!primary_key.is_empty()).then_some(primary_key),
        foreign_keys,
        indexes: Vec::new(),
        rls: None,
        triggers: Vec::new(),
        row_count_estimate: table_metadata.row_count_estimate,
        comment: table_metadata.comment,
        source_ddl: None,
        kind_info: TableKindInfo {
            kind: table_metadata.kind,
            ..TableKindInfo::default()
        },
    })
}

async fn find_table(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<MysqlTableMetadata, DbOperationError> {
    let database = selected_database(dsn)?;
    if schema != database {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    let result = execute_metadata_query(dsn, TABLES_QUERY).await?;
    parse_table_metadata(&result)?
        .into_iter()
        .find(|candidate| candidate.name == table)
        .ok_or_else(|| {
            DbOperationError::ObjectMissing(format!("MySQL table not found: {schema}.{table}"))
        })
}

async fn fetch_columns(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<Vec<MysqlColumnMetadata>, DbOperationError> {
    let query = format!(
        "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = DATABASE() AND c.TABLE_NAME = {} ORDER BY c.ORDINAL_POSITION",
        quote_string(table)
    );
    let result = execute_metadata_query(dsn, &query).await?;
    let database = selected_database(dsn)?;
    if schema != database {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    let columns = parse_column_metadata(&result)?;
    if columns.is_empty() {
        return Err(DbOperationError::MetadataParseFailed(format!(
            "MySQL object has no column metadata: {schema}.{table}"
        )));
    }
    Ok(columns)
}

async fn execute_metadata_query(
    dsn: &str,
    query: &str,
) -> Result<MysqlResultSet, DbOperationError> {
    let target = parse_mysql_dsn(dsn)?;
    validate_mysql_values(&target)?;
    super::validate_mysql_tls_files(&target)?;
    let statements = super::validate_mysql_multi_query(
        query,
        target.database.as_deref(),
        AccessMode::ReadWrite,
    )?;
    let option_file = MySqlOptionFile::create(&target)?;
    let result =
        run_mysql_adhoc(&option_file.path, query, &statements, AccessMode::ReadWrite).await;
    drop(option_file);
    result?.result_set.ok_or_else(|| {
        DbOperationError::MetadataParseFailed(
            "MySQL metadata query returned no result set".to_string(),
        )
    })
}

fn selected_database(dsn: &str) -> Result<String, DbOperationError> {
    parse_mysql_dsn(dsn)?.database.ok_or_else(|| {
        DbOperationError::UnsupportedOperation(
            "MySQL metadata requires a selected database".to_string(),
        )
    })
}

fn parse_table_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlTableMetadata>, DbOperationError> {
    expect_columns(
        result,
        &["TABLE_NAME", "TABLE_TYPE", "TABLE_ROWS", "TABLE_COMMENT"],
    )?;
    let tables = result
        .values
        .iter()
        .map(|row| {
            if row.len() != 4 {
                return Err(metadata_shape_error("TABLES row"));
            }
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
            }
            let name = required_text(&row[0], "TABLE_NAME")?.to_string();
            let kind = match required_text(&row[1], "TABLE_TYPE")? {
                "BASE TABLE" => TableKind::Table,
                "VIEW" => TableKind::View,
                value => {
                    return Err(DbOperationError::MetadataParseFailed(format!(
                        "unexpected MySQL table type: {value}"
                    )));
                }
            };
            let row_count_estimate = optional_text(&row[2], "TABLE_ROWS")?
                .map(|value| {
                    value.parse::<i64>().map_err(|_| {
                        DbOperationError::MetadataParseFailed(
                            "invalid MySQL TABLE_ROWS value".to_string(),
                        )
                    })
                })
                .transpose()?;
            let comment = optional_text(&row[3], "TABLE_COMMENT")?
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            Ok(Some(MysqlTableMetadata {
                name,
                kind,
                row_count_estimate,
                comment,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tables.into_iter().flatten().collect())
}

async fn fetch_indexes(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<Vec<Index>, DbOperationError> {
    let query = format!(
        "SELECT s.INDEX_NAME, s.NON_UNIQUE, s.INDEX_TYPE, s.SEQ_IN_INDEX, s.COLUMN_NAME, CASE WHEN tc.CONSTRAINT_TYPE = 'PRIMARY KEY' THEN 'YES' ELSE 'NO' END AS IS_PRIMARY FROM INFORMATION_SCHEMA.STATISTICS AS s LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = s.TABLE_SCHEMA AND tc.TABLE_SCHEMA = s.TABLE_SCHEMA AND tc.TABLE_NAME = s.TABLE_NAME AND tc.CONSTRAINT_NAME = s.INDEX_NAME WHERE s.TABLE_SCHEMA = DATABASE() AND s.TABLE_NAME = {} UNION ALL SELECT NULL, NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.STATISTICS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = {}) ORDER BY INDEX_NAME, SEQ_IN_INDEX",
        quote_string(table),
        quote_string(table),
    );
    let result = execute_metadata_query(dsn, &query).await?;
    let raw = parse_index_metadata(&result)?;
    if schema != selected_database(dsn)? {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    Ok(indexes_from_metadata(raw))
}

async fn fetch_foreign_keys(
    dsn: &str,
    schema: &str,
    table: &str,
    summaries: &[TableSummary],
) -> Result<Vec<ForeignKey>, DbOperationError> {
    let query = format!(
        "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = DATABASE() AND tc.TABLE_NAME = {} AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' UNION ALL SELECT NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = DATABASE() AND TABLE_SCHEMA = DATABASE() AND TABLE_NAME = {} AND CONSTRAINT_TYPE = 'FOREIGN KEY') ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION",
        quote_string(table),
        quote_string(table),
    );
    let result = execute_metadata_query(dsn, &query).await?;
    let raw = parse_foreign_key_metadata(&result)?;
    if schema != selected_database(dsn)? {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    foreign_keys_from_metadata(raw, summaries)
}

async fn fetch_triggers(
    dsn: &str,
    schema: &str,
    table: &str,
) -> Result<Vec<Trigger>, DbOperationError> {
    if schema != selected_database(dsn)? {
        return Err(DbOperationError::UnsupportedOperation(
            "MySQL metadata is limited to the selected database".to_string(),
        ));
    }
    let result = execute_metadata_query(dsn, &triggers_query(table)).await?;
    triggers_from_metadata(parse_trigger_metadata(&result)?)
}

async fn fetch_source_ddl(
    dsn: &str,
    table: &str,
    kind: TableKind,
) -> Result<String, DbOperationError> {
    let result = execute_metadata_query(dsn, &show_create_query(table, kind)).await?;
    parse_source_ddl(&result, kind)
}

fn triggers_query(table: &str) -> String {
    format!(
        "SELECT TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION, ACTION_STATEMENT, DEFINER FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = DATABASE() AND EVENT_OBJECT_SCHEMA = DATABASE() AND EVENT_OBJECT_TABLE = {} UNION ALL SELECT NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = DATABASE() AND EVENT_OBJECT_SCHEMA = DATABASE() AND EVENT_OBJECT_TABLE = {}) ORDER BY TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION",
        quote_string(table),
        quote_string(table),
    )
}

fn show_create_query(table: &str, kind: TableKind) -> String {
    let object_type = if kind == TableKind::View {
        "VIEW"
    } else {
        "TABLE"
    };
    format!("SHOW CREATE {object_type} {}", quote_identifier(table))
}

fn parse_trigger_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlTriggerMetadata>, DbOperationError> {
    expect_columns(
        result,
        &[
            "TRIGGER_NAME",
            "ACTION_TIMING",
            "EVENT_MANIPULATION",
            "ACTION_STATEMENT",
            "DEFINER",
        ],
    )?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 5 {
                return Err(metadata_shape_error("TRIGGERS row"));
            }
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
            }
            let timing = required_text(&row[1], "ACTION_TIMING")?
                .parse::<TriggerTiming>()
                .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
            let event = required_text(&row[2], "EVENT_MANIPULATION")?
                .parse::<TriggerEvent>()
                .map_err(|error| DbOperationError::MetadataParseFailed(error.to_string()))?;
            Ok(Some(MysqlTriggerMetadata {
                name: required_text(&row[0], "TRIGGER_NAME")?.to_string(),
                timing,
                event,
                definition: required_text(&row[3], "ACTION_STATEMENT")?.to_string(),
                security_context: optional_text(&row[4], "DEFINER")?.map(str::to_string),
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|rows| rows.into_iter().flatten().collect())
}

fn triggers_from_metadata(
    raw: Vec<MysqlTriggerMetadata>,
) -> Result<Vec<Trigger>, DbOperationError> {
    let mut triggers = Vec::new();
    for metadata in raw {
        if let Some(trigger) = triggers
            .iter_mut()
            .find(|trigger: &&mut Trigger| trigger.name == metadata.name)
        {
            if trigger.timing != metadata.timing
                || trigger.definition != metadata.definition
                || trigger.security_context != metadata.security_context
            {
                return Err(metadata_shape_error("TRIGGERS definition"));
            }
            trigger.events.push(metadata.event);
        } else {
            triggers.push(Trigger {
                name: metadata.name,
                timing: metadata.timing,
                events: vec![metadata.event],
                definition: metadata.definition,
                security_context: metadata.security_context,
            });
        }
    }
    Ok(triggers)
}

fn parse_source_ddl(result: &MysqlResultSet, kind: TableKind) -> Result<String, DbOperationError> {
    let expected_columns = if kind == TableKind::View {
        ["View", "Create View"]
    } else {
        ["Table", "Create Table"]
    };
    if result.columns.len() < 2
        || result.columns[0] != expected_columns[0]
        || result.columns[1] != expected_columns[1]
        || result.values.len() != 1
    {
        return Err(metadata_shape_error("SHOW CREATE result"));
    }
    let row = &result.values[0];
    if row.len() != result.columns.len() {
        return Err(metadata_shape_error("SHOW CREATE row"));
    }
    let ddl = required_text(&row[1], expected_columns[1])?;
    if ddl.is_empty() {
        return Err(DbOperationError::MetadataParseFailed(
            "MySQL SHOW CREATE returned empty DDL".to_string(),
        ));
    }
    Ok(ddl.to_string())
}

fn parse_column_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlColumnMetadata>, DbOperationError> {
    expect_columns(
        result,
        &[
            "COLUMN_NAME",
            "COLUMN_TYPE",
            "IS_NULLABLE",
            "COLUMN_DEFAULT",
            "EXTRA",
            "COLUMN_COMMENT",
            "ORDINAL_POSITION",
            "PRIMARY_KEY_POSITION",
        ],
    )?;
    result
        .values
        .iter()
        .map(|row| {
            if row.len() != 8 {
                return Err(metadata_shape_error("COLUMNS row"));
            }
            let extra = required_text(&row[4], "EXTRA")?;
            Ok(MysqlColumnMetadata {
                name: required_text(&row[0], "COLUMN_NAME")?.to_string(),
                data_type: required_text(&row[1], "COLUMN_TYPE")?.to_string(),
                nullable: required_text(&row[2], "IS_NULLABLE")? == "YES",
                default: optional_text(&row[3], "COLUMN_DEFAULT")?.map(str::to_string),
                comment: optional_text(&row[5], "COLUMN_COMMENT")?
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                ordinal_position: parse_positive_i32(&row[6], "ORDINAL_POSITION")?,
                primary_key_position: optional_text(&row[7], "PRIMARY_KEY_POSITION")?
                    .map(|value| parse_positive_i32_text(value, "PRIMARY_KEY_POSITION"))
                    .transpose()?,
                invisible: extra
                    .split_ascii_whitespace()
                    .any(|word| word.eq_ignore_ascii_case("INVISIBLE")),
                generated: extra
                    .split_ascii_whitespace()
                    .any(|word| word.eq_ignore_ascii_case("GENERATED")),
            })
        })
        .collect()
}

fn parse_index_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlIndexMetadata>, DbOperationError> {
    expect_columns(
        result,
        &[
            "INDEX_NAME",
            "NON_UNIQUE",
            "INDEX_TYPE",
            "SEQ_IN_INDEX",
            "COLUMN_NAME",
            "IS_PRIMARY",
        ],
    )?;
    result
        .values
        .iter()
        .map(|row| {
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
            }
            if row.len() != 6 {
                return Err(metadata_shape_error("STATISTICS row"));
            }
            Ok(Some(MysqlIndexMetadata {
                name: required_text(&row[0], "INDEX_NAME")?.to_string(),
                non_unique: parse_boolean_flag(&row[1], "NON_UNIQUE")?,
                index_type: required_text(&row[2], "INDEX_TYPE")?.to_string(),
                ordinal_position: parse_positive_i32(&row[3], "SEQ_IN_INDEX")?,
                column_name: required_text(&row[4], "COLUMN_NAME")?.to_string(),
                primary: parse_boolean_flag(&row[5], "IS_PRIMARY")?,
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|rows| rows.into_iter().flatten().collect())
}

fn parse_foreign_key_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlForeignKeyMetadata>, DbOperationError> {
    expect_columns(
        result,
        &[
            "CONSTRAINT_NAME",
            "TABLE_SCHEMA",
            "TABLE_NAME",
            "COLUMN_NAME",
            "REFERENCED_TABLE_SCHEMA",
            "REFERENCED_TABLE_NAME",
            "REFERENCED_COLUMN_NAME",
            "ORDINAL_POSITION",
            "UPDATE_RULE",
            "DELETE_RULE",
        ],
    )?;
    result
        .values
        .iter()
        .map(|row| {
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
            }
            if row.len() != 10 {
                return Err(metadata_shape_error("foreign key row"));
            }
            Ok(Some(MysqlForeignKeyMetadata {
                name: required_text(&row[0], "CONSTRAINT_NAME")?.to_string(),
                from_schema: required_text(&row[1], "TABLE_SCHEMA")?.to_string(),
                from_table: required_text(&row[2], "TABLE_NAME")?.to_string(),
                from_column: required_text(&row[3], "COLUMN_NAME")?.to_string(),
                to_schema: required_text(&row[4], "REFERENCED_TABLE_SCHEMA")?.to_string(),
                to_table: required_text(&row[5], "REFERENCED_TABLE_NAME")?.to_string(),
                to_column: required_text(&row[6], "REFERENCED_COLUMN_NAME")?.to_string(),
                ordinal_position: parse_positive_i32(&row[7], "ORDINAL_POSITION")?,
                on_update: parse_fk_action(&row[8], "UPDATE_RULE")?,
                on_delete: parse_fk_action(&row[9], "DELETE_RULE")?,
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|rows| rows.into_iter().flatten().collect())
}

fn indexes_from_metadata(mut raw: Vec<MysqlIndexMetadata>) -> Vec<Index> {
    raw.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.ordinal_position.cmp(&right.ordinal_position))
    });
    let mut indexes = Vec::new();
    for column in raw {
        if let Some(index) = indexes
            .iter_mut()
            .find(|index: &&mut Index| index.name == column.name)
        {
            index.columns.push(column.column_name);
            continue;
        }
        indexes.push(Index {
            name: column.name,
            columns: vec![column.column_name],
            attributes: IndexAttributes::from_parts(!column.non_unique, column.primary),
            index_type: column
                .index_type
                .to_ascii_lowercase()
                .parse::<IndexType>()
                .unwrap_or_else(|never| match never {}),
            definition: None,
        });
    }
    indexes
}

fn foreign_keys_from_metadata(
    mut raw: Vec<MysqlForeignKeyMetadata>,
    summaries: &[TableSummary],
) -> Result<Vec<ForeignKey>, DbOperationError> {
    raw.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.ordinal_position.cmp(&right.ordinal_position))
    });
    let mut foreign_keys = Vec::new();
    for column in raw {
        let reference_resolved = summaries
            .iter()
            .any(|summary| summary.schema == column.to_schema && summary.name == column.to_table);
        if let Some(foreign_key) = foreign_keys
            .iter_mut()
            .find(|foreign_key: &&mut ForeignKey| foreign_key.name == column.name)
        {
            if foreign_key.from_schema != column.from_schema
                || foreign_key.from_table != column.from_table
                || foreign_key.to_schema != column.to_schema
                || foreign_key.to_table != column.to_table
                || foreign_key.on_update != column.on_update
                || foreign_key.on_delete != column.on_delete
            {
                return Err(metadata_shape_error("foreign key constraint columns"));
            }
            foreign_key.from_columns.push(column.from_column);
            foreign_key.to_columns.push(column.to_column);
            foreign_key.reference_resolved &= reference_resolved;
            continue;
        }
        foreign_keys.push(ForeignKey {
            name: column.name,
            from_schema: column.from_schema,
            from_table: column.from_table,
            from_columns: vec![column.from_column],
            to_schema: column.to_schema,
            to_table: column.to_table,
            to_columns: vec![column.to_column],
            on_delete: column.on_delete,
            on_update: column.on_update,
            reference_resolved,
        });
    }
    Ok(foreign_keys)
}

fn column_from_metadata(metadata: &MysqlColumnMetadata) -> Column {
    let primary_key = metadata.primary_key_position.is_some();
    let attributes = ColumnAttributes::from_parts(metadata.nullable, primary_key, false)
        | if metadata.invisible {
            ColumnAttributes::HIDDEN | ColumnAttributes::READ_ONLY
        } else {
            ColumnAttributes::empty()
        }
        | if metadata.generated {
            ColumnAttributes::GENERATED | ColumnAttributes::READ_ONLY
        } else {
            ColumnAttributes::empty()
        };
    Column {
        name: metadata.name.clone(),
        data_type: metadata.data_type.clone(),
        default: metadata.default.clone(),
        attributes,
        comment: metadata.comment.clone(),
        ordinal_position: metadata.ordinal_position,
    }
}

fn primary_key_names(columns: &[MysqlColumnMetadata]) -> Vec<String> {
    let mut columns = columns
        .iter()
        .filter_map(|column| {
            column
                .primary_key_position
                .map(|position| (position, column.name.clone()))
        })
        .collect::<Vec<_>>();
    columns.sort_by_key(|(position, _)| *position);
    columns.into_iter().map(|(_, name)| name).collect()
}

fn parse_boolean_flag(value: &QueryValue, field: &str) -> Result<bool, DbOperationError> {
    match required_text(value, field)? {
        "0" | "NO" => Ok(false),
        "1" | "YES" => Ok(true),
        _ => Err(DbOperationError::MetadataParseFailed(format!(
            "invalid MySQL metadata boolean: {field}"
        ))),
    }
}

fn parse_fk_action(value: &QueryValue, field: &str) -> Result<FkAction, DbOperationError> {
    required_text(value, field)?.parse().map_err(|error| {
        DbOperationError::MetadataParseFailed(format!("invalid MySQL foreign key action: {error}"))
    })
}

fn table_signature(table: &Table) -> String {
    format!(
        "{:?}|{:?}|{:?}",
        table.kind_info.kind, table.columns, table.foreign_keys
    )
}

fn table_summary(database: &str, table: MysqlTableMetadata) -> TableSummary {
    TableSummary::new(
        database.to_string(),
        table.name,
        table.row_count_estimate,
        false,
    )
    .with_kind_info(TableKindInfo {
        kind: table.kind,
        ..TableKindInfo::default()
    })
}

fn expect_columns(result: &MysqlResultSet, expected: &[&str]) -> Result<(), DbOperationError> {
    if result.columns == expected {
        Ok(())
    } else {
        Err(metadata_shape_error("MySQL metadata columns"))
    }
}

fn required_text<'a>(value: &'a QueryValue, field: &str) -> Result<&'a str, DbOperationError> {
    optional_text(value, field)?.ok_or_else(|| {
        DbOperationError::MetadataParseFailed(format!("MySQL metadata field is NULL: {field}"))
    })
}

fn optional_text<'a>(
    value: &'a QueryValue,
    field: &str,
) -> Result<Option<&'a str>, DbOperationError> {
    match value {
        QueryValue::Null => Ok(None),
        QueryValue::Text(value) | QueryValue::SqlLiteral(value) => Ok(Some(value)),
        QueryValue::Blob(_) => Err(DbOperationError::MetadataParseFailed(format!(
            "MySQL metadata field is binary: {field}"
        ))),
    }
}

fn parse_positive_i32(value: &QueryValue, field: &str) -> Result<i32, DbOperationError> {
    parse_positive_i32_text(required_text(value, field)?, field)
}

fn parse_positive_i32_text(value: &str, field: &str) -> Result<i32, DbOperationError> {
    let value = value.parse::<i32>().map_err(|_| {
        DbOperationError::MetadataParseFailed(format!("invalid MySQL metadata integer: {field}"))
    })?;
    if value > 0 {
        Ok(value)
    } else {
        Err(DbOperationError::MetadataParseFailed(format!(
            "invalid MySQL metadata ordinal: {field}"
        )))
    }
}

fn metadata_shape_error(field: &str) -> DbOperationError {
    DbOperationError::MetadataParseFailed(format!("unexpected MySQL metadata shape: {field}"))
}

fn quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn quote_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

fn is_binary_type(data_type: &str) -> bool {
    let name = data_type
        .split(['(', ' '])
        .next()
        .unwrap_or(data_type)
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "binary" | "varbinary" | "tinyblob" | "blob" | "mediumblob" | "longblob" | "bit"
    )
}

fn is_numeric_type(data_type: &str) -> bool {
    let name = data_type
        .split(['(', ' '])
        .next()
        .unwrap_or(data_type)
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "tinyint"
            | "smallint"
            | "mediumint"
            | "int"
            | "integer"
            | "bigint"
            | "decimal"
            | "dec"
            | "numeric"
            | "fixed"
            | "float"
            | "double"
            | "real"
    )
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let hex = value.strip_prefix("0x")?;
    if hex.len() % 2 != 0 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect()
}

fn is_sql_numeric_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = usize::from(matches!(bytes[0], b'+' | b'-'));
    let integer_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let has_integer = index > integer_start;
    let mut has_fraction = false;
    if bytes.get(index) == Some(&b'.') {
        has_fraction = true;
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if !has_integer && !has_fraction {
        return false;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if exponent_start == index {
            return false;
        }
    }
    index == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MysqlResultSet {
        MysqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }

    fn column(name: &str, data_type: &str) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            default: None,
            attributes: ColumnAttributes::empty(),
            comment: None,
            ordinal_position: 1,
        }
    }

    #[test]
    fn trigger_metadata_preserves_action_definer_and_event_order() {
        let result = result(
            &[
                "TRIGGER_NAME",
                "ACTION_TIMING",
                "EVENT_MANIPULATION",
                "ACTION_STATEMENT",
                "DEFINER",
            ],
            vec![
                vec![
                    QueryValue::Text("audit_changes".to_string()),
                    QueryValue::Text("BEFORE".to_string()),
                    QueryValue::Text("INSERT".to_string()),
                    QueryValue::Text("BEGIN\n  SET @seen = 1;\nEND".to_string()),
                    QueryValue::Text("sabiql@%".to_string()),
                ],
                vec![
                    QueryValue::Text("audit_changes".to_string()),
                    QueryValue::Text("BEFORE".to_string()),
                    QueryValue::Text("UPDATE".to_string()),
                    QueryValue::Text("BEGIN\n  SET @seen = 1;\nEND".to_string()),
                    QueryValue::Text("sabiql@%".to_string()),
                ],
            ],
        );

        let triggers = triggers_from_metadata(parse_trigger_metadata(&result).unwrap()).unwrap();

        assert_eq!(triggers.len(), 1);
        assert_eq!(
            triggers[0].events,
            [TriggerEvent::Insert, TriggerEvent::Update]
        );
        assert_eq!(triggers[0].definition, "BEGIN\n  SET @seen = 1;\nEND");
        assert_eq!(triggers[0].security_context.as_deref(), Some("sabiql@%"));
    }

    #[test]
    fn empty_trigger_sentinel_returns_no_triggers() {
        let result = result(
            &[
                "TRIGGER_NAME",
                "ACTION_TIMING",
                "EVENT_MANIPULATION",
                "ACTION_STATEMENT",
                "DEFINER",
            ],
            vec![vec![
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
            ]],
        );

        assert!(parse_trigger_metadata(&result).unwrap().is_empty());
    }

    #[test]
    fn show_create_query_uses_object_kind_and_identifier_quoting() {
        assert_eq!(
            show_create_query("table`name", TableKind::Table),
            "SHOW CREATE TABLE `table``name`"
        );
        assert_eq!(
            show_create_query("view_name", TableKind::View),
            "SHOW CREATE VIEW `view_name`"
        );
    }

    #[test]
    fn show_create_parser_preserves_view_ddl_and_extra_server_columns() {
        let ddl = "CREATE ALGORITHM=UNDEFINED VIEW `v` AS select 1";
        let result = result(
            &[
                "View",
                "Create View",
                "character_set_client",
                "collation_connection",
            ],
            vec![vec![
                QueryValue::Text("v".to_string()),
                QueryValue::Text(ddl.to_string()),
                QueryValue::Text("utf8mb4".to_string()),
                QueryValue::Text("utf8mb4_0900_ai_ci".to_string()),
            ]],
        );

        assert_eq!(parse_source_ddl(&result, TableKind::View).unwrap(), ddl);
    }

    #[test]
    fn preview_conversion_keeps_text_hex_and_decodes_binary_hex() {
        let result = result(
            &["text_value", "binary_value"],
            vec![vec![
                QueryValue::Text("0x41".to_string()),
                QueryValue::Text("0x00FF".to_string()),
            ]],
        );
        let columns = vec![
            column("text_value", "varchar(20)"),
            column("binary_value", "blob"),
        ];

        let values = convert_preview_values(&result, &columns, &[]).expect("conversion succeeds");

        assert_eq!(values.visible[0][0], QueryValue::Text("0x41".to_string()));
        assert_eq!(values.visible[0][1], QueryValue::Blob(vec![0, 255]));
    }

    #[test]
    fn preview_conversion_keeps_numeric_server_literals_without_rounding() {
        let result = result(
            &["unsigned_value", "decimal_value", "float_value"],
            vec![vec![
                QueryValue::Text("18446744073709551615".to_string()),
                QueryValue::Text("12345678901234567890.12345678901234567890".to_string()),
                QueryValue::Text("1.23e+100".to_string()),
            ]],
        );
        let columns = vec![
            column("unsigned_value", "bigint unsigned"),
            column("decimal_value", "decimal(65,30)"),
            column("float_value", "double"),
        ];

        let values = convert_preview_values(&result, &columns, &[]).expect("conversion succeeds");

        assert_eq!(values.visible[0][0].as_str(), Some("18446744073709551615"));
        assert!(matches!(values.visible[0][0], QueryValue::SqlLiteral(_)));
        assert!(matches!(values.visible[0][1], QueryValue::SqlLiteral(_)));
        assert!(matches!(values.visible[0][2], QueryValue::SqlLiteral(_)));
    }

    #[test]
    fn numeric_literal_validation_rejects_partial_values() {
        assert!(is_sql_numeric_literal("1."));
        assert!(is_sql_numeric_literal(".5"));
        assert!(is_sql_numeric_literal("-1.2e-3"));
        assert!(!is_sql_numeric_literal("1e"));
        assert!(!is_sql_numeric_literal("nan"));
    }

    #[test]
    fn preview_query_lists_visible_columns_and_orders_by_primary_key() {
        let columns = vec![column("id", "int"), column("display", "text")];

        let query = build_preview_query(
            "app",
            "items",
            &["id".to_string()],
            &columns,
            &[],
            500,
            1000,
        );

        assert_eq!(
            query,
            "SELECT `id`, `display` FROM `app`.`items` ORDER BY `id` LIMIT 500 OFFSET 1000"
        );
    }

    #[test]
    fn preview_query_appends_aliased_hidden_primary_key_columns() {
        let visible_columns = vec![column("payload", "text")];
        let identity_columns = vec![column("id", "int")];

        let query = build_preview_query(
            "app",
            "items",
            &["id".to_string()],
            &visible_columns,
            &identity_columns,
            10,
            0,
        );

        assert_eq!(
            query,
            "SELECT `payload`, `id` AS `__sabiql_row_identity_0` FROM `app`.`items` ORDER BY `id` LIMIT 10 OFFSET 0"
        );
    }

    #[test]
    fn preview_conversion_separates_aliased_primary_key_values() {
        let result = result(
            &["payload", "__sabiql_row_identity_0"],
            vec![vec![
                QueryValue::Text("visible".to_string()),
                QueryValue::Text("42".to_string()),
            ]],
        );
        let visible_columns = vec![column("payload", "text")];
        let identity_columns = vec![column("id", "int")];

        let values = convert_preview_values(&result, &visible_columns, &identity_columns)
            .expect("conversion succeeds");

        assert_eq!(
            values.visible,
            vec![vec![QueryValue::Text("visible".to_string())]]
        );
        assert_eq!(
            values.identity,
            Some(vec![vec![QueryValue::SqlLiteral("42".to_string())]])
        );
    }

    #[test]
    fn metadata_parser_preserves_column_and_composite_key_order() {
        let result = result(
            &[
                "COLUMN_NAME",
                "COLUMN_TYPE",
                "IS_NULLABLE",
                "COLUMN_DEFAULT",
                "EXTRA",
                "COLUMN_COMMENT",
                "ORDINAL_POSITION",
                "PRIMARY_KEY_POSITION",
            ],
            vec![
                vec![
                    QueryValue::Text("first_key".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Text(String::new()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("2".to_string()),
                ],
                vec![
                    QueryValue::Text("second_key".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Text(String::new()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("1".to_string()),
                ],
                vec![
                    QueryValue::Text("generated_value".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("YES".to_string()),
                    QueryValue::Null,
                    QueryValue::Text("STORED GENERATED".to_string()),
                    QueryValue::Text(String::new()),
                    QueryValue::Text("3".to_string()),
                    QueryValue::Null,
                ],
            ],
        );

        let parsed = parse_column_metadata(&result).expect("metadata parses");
        assert_eq!(primary_key_names(&parsed), ["second_key", "first_key"]);
        assert_eq!(
            parsed
                .iter()
                .map(|column| column.ordinal_position)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(column_from_metadata(&parsed[2]).is_generated());
    }

    #[test]
    fn empty_table_list_sentinel_parses_as_no_tables() {
        let result = result(
            &["TABLE_NAME", "TABLE_TYPE", "TABLE_ROWS", "TABLE_COMMENT"],
            vec![vec![
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
                QueryValue::Null,
            ]],
        );

        assert!(
            parse_table_metadata(&result)
                .expect("empty table metadata parses")
                .is_empty()
        );
    }

    #[test]
    fn groups_indexes_by_name_and_orders_columns_by_sequence() {
        let result = result(
            &[
                "INDEX_NAME",
                "NON_UNIQUE",
                "INDEX_TYPE",
                "SEQ_IN_INDEX",
                "COLUMN_NAME",
                "IS_PRIMARY",
            ],
            vec![
                vec![
                    QueryValue::Text("PRIMARY".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("second_key".to_string()),
                    QueryValue::Text("YES".to_string()),
                ],
                vec![
                    QueryValue::Text("PRIMARY".to_string()),
                    QueryValue::Text("0".to_string()),
                    QueryValue::Text("BTREE".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("first_key".to_string()),
                    QueryValue::Text("YES".to_string()),
                ],
                vec![
                    QueryValue::Text("search_index".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("FULLTEXT".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("body".to_string()),
                    QueryValue::Text("NO".to_string()),
                ],
            ],
        );

        let indexes = indexes_from_metadata(parse_index_metadata(&result).unwrap());

        assert_eq!(indexes[0].name, "PRIMARY");
        assert_eq!(indexes[0].columns, ["first_key", "second_key"]);
        assert!(indexes[0].is_primary());
        assert_eq!(
            indexes[1].index_type,
            IndexType::Other("fulltext".to_string())
        );
        assert!(!indexes[1].is_unique());
    }

    #[test]
    fn groups_foreign_keys_by_name_and_orders_columns_by_sequence() {
        let result = result(
            &[
                "CONSTRAINT_NAME",
                "TABLE_SCHEMA",
                "TABLE_NAME",
                "COLUMN_NAME",
                "REFERENCED_TABLE_SCHEMA",
                "REFERENCED_TABLE_NAME",
                "REFERENCED_COLUMN_NAME",
                "ORDINAL_POSITION",
                "UPDATE_RULE",
                "DELETE_RULE",
            ],
            vec![
                vec![
                    QueryValue::Text("fk_child_parent".to_string()),
                    QueryValue::Text("sabiql_test".to_string()),
                    QueryValue::Text("child".to_string()),
                    QueryValue::Text("parent_second".to_string()),
                    QueryValue::Text("sabiql_test".to_string()),
                    QueryValue::Text("parent".to_string()),
                    QueryValue::Text("second_key".to_string()),
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("CASCADE".to_string()),
                    QueryValue::Text("SET NULL".to_string()),
                ],
                vec![
                    QueryValue::Text("fk_child_parent".to_string()),
                    QueryValue::Text("sabiql_test".to_string()),
                    QueryValue::Text("child".to_string()),
                    QueryValue::Text("parent_first".to_string()),
                    QueryValue::Text("sabiql_test".to_string()),
                    QueryValue::Text("parent".to_string()),
                    QueryValue::Text("first_key".to_string()),
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("CASCADE".to_string()),
                    QueryValue::Text("SET NULL".to_string()),
                ],
            ],
        );

        let summaries = [TableSummary::new(
            "sabiql_test".to_string(),
            "parent".to_string(),
            None,
            false,
        )];
        let foreign_keys =
            foreign_keys_from_metadata(parse_foreign_key_metadata(&result).unwrap(), &summaries)
                .unwrap();

        assert_eq!(foreign_keys.len(), 1);
        assert_eq!(
            foreign_keys[0].from_columns,
            ["parent_first", "parent_second"]
        );
        assert_eq!(foreign_keys[0].to_columns, ["first_key", "second_key"]);
        assert_eq!(foreign_keys[0].on_update, FkAction::Cascade);
        assert_eq!(foreign_keys[0].on_delete, FkAction::SetNull);

        let unresolved =
            foreign_keys_from_metadata(parse_foreign_key_metadata(&result).unwrap(), &[]).unwrap();
        assert!(!unresolved[0].is_reference_resolved());
    }
}
