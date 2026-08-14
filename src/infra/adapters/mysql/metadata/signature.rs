use std::collections::{HashMap, HashSet};

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{QueryValue, Table, TableKindInfo, TableSignature, TableSummary};

use super::super::cli::MysqlResultSet;
use super::catalog::{
    MysqlColumnMetadata, MysqlForeignKeyMetadata, MysqlTableMetadata, column_from_metadata,
    execute_metadata_query, expect_columns, fetch_metadata_snapshot, foreign_keys_from_metadata,
    metadata_shape_error, parse_column_metadata_row, parse_foreign_key_metadata, primary_key_names,
    required_text,
};

const SIGNATURE_COLUMNS_QUERY: &str = "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = DATABASE() UNION ALL SELECT NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = DATABASE()) ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION";
const SIGNATURE_FOREIGN_KEYS_QUERY: &str = "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = DATABASE() AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' UNION ALL SELECT NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = DATABASE() AND TABLE_SCHEMA = DATABASE() AND CONSTRAINT_TYPE = 'FOREIGN KEY') ORDER BY TABLE_SCHEMA, TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION";

#[derive(Debug, Clone)]
struct MysqlSignatureColumnMetadata {
    schema: String,
    table: String,
    column: MysqlColumnMetadata,
}

pub(super) async fn fetch_table_signatures(
    dsn: &str,
) -> Result<Vec<TableSignature>, DbOperationError> {
    let snapshot = fetch_metadata_snapshot(dsn).await?;
    let columns = execute_metadata_query(dsn, SIGNATURE_COLUMNS_QUERY).await?;
    let foreign_keys = execute_metadata_query(dsn, SIGNATURE_FOREIGN_KEYS_QUERY).await?;
    table_signatures_from_metadata(
        &snapshot.tables,
        &snapshot.table_summaries,
        parse_signature_column_metadata(&columns)?,
        parse_foreign_key_metadata(&foreign_keys)?,
    )
}

fn parse_signature_column_metadata(
    result: &MysqlResultSet,
) -> Result<Vec<MysqlSignatureColumnMetadata>, DbOperationError> {
    expect_columns(
        result,
        &[
            "TABLE_SCHEMA",
            "TABLE_NAME",
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
            if row.len() != 10 {
                return Err(metadata_shape_error("signature COLUMNS row"));
            }
            if row.iter().all(|value| matches!(value, QueryValue::Null)) {
                return Ok(None);
            }
            Ok(Some(MysqlSignatureColumnMetadata {
                schema: required_text(&row[0], "TABLE_SCHEMA")?.to_string(),
                table: required_text(&row[1], "TABLE_NAME")?.to_string(),
                column: parse_column_metadata_row(&row[2..], "signature COLUMNS row")?,
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|rows| rows.into_iter().flatten().collect())
}

fn table_signatures_from_metadata(
    tables: &[MysqlTableMetadata],
    summaries: &[TableSummary],
    columns: Vec<MysqlSignatureColumnMetadata>,
    foreign_keys: Vec<MysqlForeignKeyMetadata>,
) -> Result<Vec<TableSignature>, DbOperationError> {
    let known_tables: HashSet<(String, String)> = tables
        .iter()
        .map(|table| (table.schema.clone(), table.name.clone()))
        .collect();
    let mut columns_by_table: HashMap<(String, String), Vec<MysqlColumnMetadata>> = HashMap::new();
    for metadata in columns {
        let key = (metadata.schema.clone(), metadata.table.clone());
        if !known_tables.contains(&key) {
            return Err(DbOperationError::MetadataParseFailed(format!(
                "MySQL signature metadata references unknown table: {}.{}",
                metadata.schema, metadata.table
            )));
        }
        columns_by_table
            .entry(key)
            .or_default()
            .push(metadata.column);
    }

    let mut foreign_keys_by_table: HashMap<(String, String), Vec<MysqlForeignKeyMetadata>> =
        HashMap::new();
    for foreign_key in foreign_keys {
        let key = (
            foreign_key.from_schema.clone(),
            foreign_key.from_table.clone(),
        );
        if !known_tables.contains(&key) {
            return Err(DbOperationError::MetadataParseFailed(format!(
                "MySQL signature metadata references unknown table: {}.{}",
                foreign_key.from_schema, foreign_key.from_table
            )));
        }
        foreign_keys_by_table
            .entry(key)
            .or_default()
            .push(foreign_key);
    }

    tables
        .iter()
        .map(|table| {
            let key = (table.schema.clone(), table.name.clone());
            let mut columns = columns_by_table.remove(&key).ok_or_else(|| {
                DbOperationError::MetadataParseFailed(format!(
                    "MySQL object has no column metadata: {}.{}",
                    table.schema, table.name
                ))
            })?;
            if columns.is_empty() {
                return Err(DbOperationError::MetadataParseFailed(format!(
                    "MySQL object has no column metadata: {}.{}",
                    table.schema, table.name
                )));
            }
            columns.sort_by_key(|column| column.ordinal_position);
            let primary_key = primary_key_names(&columns);
            let foreign_keys = foreign_keys_from_metadata(
                foreign_keys_by_table.remove(&key).unwrap_or_default(),
                summaries,
            )?;
            let detail = Table {
                schema: table.schema.clone(),
                name: table.name.clone(),
                owner: None,
                columns: columns.iter().map(column_from_metadata).collect(),
                primary_key: (!primary_key.is_empty()).then_some(primary_key),
                foreign_keys,
                indexes: Vec::new(),
                rls: None,
                triggers: Vec::new(),
                row_count_estimate: table.row_count_estimate,
                comment: table.comment.clone(),
                source_ddl: None,
                kind_info: TableKindInfo {
                    kind: table.kind,
                    ..TableKindInfo::default()
                },
            };
            Ok(TableSignature {
                schema: table.schema.clone(),
                name: table.name.clone(),
                signature: table_signature(&detail),
            })
        })
        .collect()
}

fn table_signature(table: &Table) -> String {
    format!(
        "{:?}|{:?}|{:?}",
        table.kind_info.kind, table.columns, table.foreign_keys
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TableKind;

    fn result(columns: &[&str], values: Vec<Vec<QueryValue>>) -> MysqlResultSet {
        MysqlResultSet {
            columns: columns.iter().map(|value| (*value).to_string()).collect(),
            values,
        }
    }

    fn table_summary(table: MysqlTableMetadata) -> TableSummary {
        TableSummary::new(table.schema, table.name, table.row_count_estimate, false).with_kind_info(
            TableKindInfo {
                kind: table.kind,
                ..TableKindInfo::default()
            },
        )
    }

    #[test]
    fn signature_metadata_batches_columns_and_foreign_keys_by_server_table() {
        let tables = vec![
            MysqlTableMetadata {
                schema: "App".to_string(),
                name: "child".to_string(),
                kind: TableKind::Table,
                row_count_estimate: None,
                comment: None,
            },
            MysqlTableMetadata {
                schema: "App".to_string(),
                name: "parent".to_string(),
                kind: TableKind::Table,
                row_count_estimate: None,
                comment: None,
            },
        ];
        let summaries = tables
            .iter()
            .cloned()
            .map(table_summary)
            .collect::<Vec<_>>();
        let columns = parse_signature_column_metadata(&result(
            &[
                "TABLE_SCHEMA",
                "TABLE_NAME",
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
                    QueryValue::Text("App".to_string()),
                    QueryValue::Text("child".to_string()),
                    QueryValue::Text("parent_id".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Null,
                    QueryValue::Text("2".to_string()),
                    QueryValue::Text("1".to_string()),
                ],
                vec![
                    QueryValue::Text("App".to_string()),
                    QueryValue::Text("child".to_string()),
                    QueryValue::Text("id".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Null,
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("2".to_string()),
                ],
                vec![
                    QueryValue::Text("App".to_string()),
                    QueryValue::Text("parent".to_string()),
                    QueryValue::Text("id".to_string()),
                    QueryValue::Text("int".to_string()),
                    QueryValue::Text("NO".to_string()),
                    QueryValue::Null,
                    QueryValue::Text(String::new()),
                    QueryValue::Null,
                    QueryValue::Text("1".to_string()),
                    QueryValue::Text("1".to_string()),
                ],
            ],
        ))
        .unwrap();
        let foreign_keys = parse_foreign_key_metadata(&result(
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
            vec![vec![
                QueryValue::Text("fk_child_parent".to_string()),
                QueryValue::Text("App".to_string()),
                QueryValue::Text("child".to_string()),
                QueryValue::Text("parent_id".to_string()),
                QueryValue::Text("App".to_string()),
                QueryValue::Text("parent".to_string()),
                QueryValue::Text("id".to_string()),
                QueryValue::Text("1".to_string()),
                QueryValue::Text("CASCADE".to_string()),
                QueryValue::Text("SET NULL".to_string()),
            ]],
        ))
        .unwrap();

        let signatures =
            table_signatures_from_metadata(&tables, &summaries, columns, foreign_keys).unwrap();

        assert_eq!(signatures.len(), 2);
        assert_eq!(signatures[0].qualified_name(), "App.child");
        assert!(signatures[0].signature.contains("id"));
        assert!(signatures[0].signature.contains("fk_child_parent"));
    }

    #[test]
    fn signature_metadata_requires_columns_for_each_table() {
        let tables = vec![MysqlTableMetadata {
            schema: "app".to_string(),
            name: "users".to_string(),
            kind: TableKind::Table,
            row_count_estimate: None,
            comment: None,
        }];
        let summaries = tables
            .iter()
            .cloned()
            .map(table_summary)
            .collect::<Vec<_>>();

        let error = table_signatures_from_metadata(&tables, &summaries, Vec::new(), Vec::new())
            .unwrap_err();

        assert!(matches!(
            error,
            DbOperationError::MetadataParseFailed(message)
                if message.contains("no column metadata: app.users")
        ));
    }
}
