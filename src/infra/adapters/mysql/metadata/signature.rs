use std::collections::{HashMap, HashSet};

use crate::app::ports::outbound::DbOperationError;
use crate::domain::{
    FkAction, QueryValue, Table, TableKind, TableKindInfo, TableSignature, TableSummary,
};

use super::super::cli::MysqlResultSet;
use super::catalog::{
    MysqlColumnMetadata, MysqlForeignKeyMetadata, MysqlTableMetadata, column_from_metadata,
    execute_metadata_query, expect_columns, fetch_metadata_snapshot, foreign_keys_from_metadata,
    metadata_shape_error, parse_column_metadata_row, parse_foreign_key_metadata, primary_key_names,
    required_text,
};

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
                    is_strict: false,
                    without_rowid: false,
                    virtual_module: None,
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
    let mut signature = String::new();
    append_record(&mut signature, "table");
    append_value(&mut signature, &table.schema);
    append_value(&mut signature, &table.name);
    append_value(&mut signature, table_kind_name(table.kind_info.kind));

    append_record(&mut signature, "primary-key");
    match &table.primary_key {
        Some(columns) => {
            append_value(&mut signature, "some");
            append_values(&mut signature, columns);
        }
        None => append_value(&mut signature, "none"),
    }

    let mut columns = table.columns.iter().collect::<Vec<_>>();
    columns.sort_by(|left, right| {
        left.ordinal_position
            .cmp(&right.ordinal_position)
            .then_with(|| left.name.cmp(&right.name))
    });
    for column in columns {
        append_record(&mut signature, "column");
        append_value(&mut signature, &column.name);
        append_value(&mut signature, &column.data_type);
        append_bool(&mut signature, column.is_nullable());
        append_optional_value(&mut signature, column.default.as_deref());
        append_optional_value(&mut signature, column.comment.as_deref());
        append_value(&mut signature, &column.ordinal_position.to_string());
        append_bool(&mut signature, column.is_primary_key());
        append_bool(&mut signature, column.is_unique());
        append_bool(&mut signature, column.is_read_only());
        append_bool(&mut signature, column.is_hidden());
        append_bool(&mut signature, column.is_generated());
    }

    let mut foreign_keys = table.foreign_keys.iter().collect::<Vec<_>>();
    foreign_keys.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.from_schema.cmp(&right.from_schema))
            .then_with(|| left.from_table.cmp(&right.from_table))
            .then_with(|| left.from_columns.cmp(&right.from_columns))
            .then_with(|| left.to_schema.cmp(&right.to_schema))
            .then_with(|| left.to_table.cmp(&right.to_table))
            .then_with(|| left.to_columns.cmp(&right.to_columns))
            .then_with(|| {
                foreign_key_action_name(&left.on_delete)
                    .cmp(foreign_key_action_name(&right.on_delete))
            })
            .then_with(|| {
                foreign_key_action_name(&left.on_update)
                    .cmp(foreign_key_action_name(&right.on_update))
            })
            .then_with(|| left.reference_resolved.cmp(&right.reference_resolved))
    });
    for foreign_key in foreign_keys {
        append_record(&mut signature, "foreign-key");
        append_value(&mut signature, &foreign_key.name);
        append_value(&mut signature, &foreign_key.from_schema);
        append_value(&mut signature, &foreign_key.from_table);
        append_values(&mut signature, &foreign_key.from_columns);
        append_value(&mut signature, &foreign_key.to_schema);
        append_value(&mut signature, &foreign_key.to_table);
        append_values(&mut signature, &foreign_key.to_columns);
        append_value(
            &mut signature,
            foreign_key_action_name(&foreign_key.on_delete),
        );
        append_value(
            &mut signature,
            foreign_key_action_name(&foreign_key.on_update),
        );
        append_bool(&mut signature, foreign_key.reference_resolved);
    }

    signature
}

fn append_record(signature: &mut String, name: &str) {
    signature.push_str(name);
    signature.push('|');
}

fn append_value(signature: &mut String, value: &str) {
    signature.push_str(&value.len().to_string());
    signature.push(':');
    signature.push_str(value);
    signature.push('|');
}

fn append_values(signature: &mut String, values: &[String]) {
    append_value(signature, &values.len().to_string());
    for value in values {
        append_value(signature, value);
    }
}

fn append_optional_value(signature: &mut String, value: Option<&str>) {
    match value {
        Some(value) => {
            append_value(signature, "some");
            append_value(signature, value);
        }
        None => append_value(signature, "none"),
    }
}

fn append_bool(signature: &mut String, value: bool) {
    append_value(signature, if value { "true" } else { "false" });
}

fn table_kind_name(kind: TableKind) -> &'static str {
    match kind {
        TableKind::Table => "table",
        TableKind::Virtual => "virtual",
        TableKind::View => "view",
    }
}

fn foreign_key_action_name(action: &FkAction) -> &'static str {
    match action {
        FkAction::NoAction => "no-action",
        FkAction::Restrict => "restrict",
        FkAction::Cascade => "cascade",
        FkAction::SetNull => "set-null",
        FkAction::SetDefault => "set-default",
    }
}

const SIGNATURE_COLUMNS_QUERY: &str = "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = DATABASE() UNION ALL SELECT NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = DATABASE()) ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION";
const SIGNATURE_FOREIGN_KEYS_QUERY: &str = "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = DATABASE() AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' UNION ALL SELECT NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL FROM DUAL WHERE NOT EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = DATABASE() AND TABLE_SCHEMA = DATABASE() AND CONSTRAINT_TYPE = 'FOREIGN KEY') ORDER BY TABLE_SCHEMA, TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, ColumnAttributes, ForeignKey};

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
                is_strict: false,
                without_rowid: false,
                virtual_module: None,
            },
        )
    }

    fn signature_table() -> Table {
        Table {
            schema: "app".to_string(),
            name: "child".to_string(),
            owner: None,
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    default: None,
                    attributes: ColumnAttributes::from_parts(false, true, false),
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "parent_id".to_string(),
                    data_type: "bigint".to_string(),
                    default: Some("0".to_string()),
                    attributes: ColumnAttributes::from_parts(true, false, false),
                    comment: Some("parent reference".to_string()),
                    ordinal_position: 2,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![
                ForeignKey {
                    name: "fk_z_parent".to_string(),
                    from_schema: "app".to_string(),
                    from_table: "child".to_string(),
                    from_columns: vec!["parent_id".to_string()],
                    to_schema: "app".to_string(),
                    to_table: "parents".to_string(),
                    to_columns: vec!["id".to_string()],
                    on_delete: FkAction::SetNull,
                    on_update: FkAction::Cascade,
                    reference_resolved: true,
                },
                ForeignKey {
                    name: "fk_a_audit".to_string(),
                    from_schema: "app".to_string(),
                    from_table: "child".to_string(),
                    from_columns: vec!["id".to_string()],
                    to_schema: "audit".to_string(),
                    to_table: "entries".to_string(),
                    to_columns: vec!["id".to_string()],
                    on_delete: FkAction::Restrict,
                    on_update: FkAction::NoAction,
                    reference_resolved: true,
                },
            ],
            indexes: Vec::new(),
            rls: None,
            triggers: Vec::new(),
            row_count_estimate: None,
            comment: None,
            source_ddl: None,
            kind_info: TableKindInfo {
                kind: TableKind::Table,
                is_strict: false,
                without_rowid: false,
                virtual_module: None,
            },
        }
    }

    fn assert_signature_changes(change: impl FnOnce(&mut Table)) {
        let mut changed = signature_table();
        let original = table_signature(&changed);
        change(&mut changed);

        assert_ne!(original, table_signature(&changed));
    }

    #[test]
    fn table_signature_does_not_depend_on_debug_output() {
        let table = signature_table();
        let signature = table_signature(&table);

        assert!(signature.contains("fk_a_audit"));
        assert!(!signature.contains("Column {"));
        assert!(!signature.contains("ForeignKey {"));
        assert!(!signature.contains("ColumnAttributes"));

        let mut unrelated = table;
        unrelated.owner = Some("owner".to_string());
        unrelated.comment = Some("display comment".to_string());
        unrelated.source_ddl = Some("CREATE TABLE child (...)".to_string());
        unrelated.row_count_estimate = Some(42);
        unrelated.kind_info.is_strict = true;
        unrelated.kind_info.without_rowid = true;
        unrelated.kind_info.virtual_module = Some("module".to_string());
        assert_eq!(signature, table_signature(&unrelated));
    }

    #[test]
    fn table_signature_is_independent_of_metadata_fetch_order() {
        let mut reordered = signature_table();
        reordered.columns.reverse();
        reordered.foreign_keys.reverse();

        assert_eq!(
            table_signature(&signature_table()),
            table_signature(&reordered)
        );
    }

    #[test]
    fn table_signature_changes_for_semantic_metadata() {
        assert_signature_changes(|table| table.schema = "other".to_string());
        assert_signature_changes(|table| table.name = "other".to_string());
        assert_signature_changes(|table| table.kind_info.kind = TableKind::View);
        assert_signature_changes(|table| table.columns[0].name = "identifier".to_string());
        assert_signature_changes(|table| table.columns[0].data_type = "bigint".to_string());
        assert_signature_changes(|table| {
            table.columns[0].attributes = ColumnAttributes::from_parts(true, true, false);
        });
        assert_signature_changes(|table| table.primary_key = Some(vec!["parent_id".to_string()]));
        assert_signature_changes(|table| table.foreign_keys[0].to_table = "accounts".to_string());
        assert_signature_changes(|table| {
            table.foreign_keys[0].from_columns = vec!["id".to_string()];
        });
        assert_signature_changes(|table| table.foreign_keys[0].on_delete = FkAction::Cascade);
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
