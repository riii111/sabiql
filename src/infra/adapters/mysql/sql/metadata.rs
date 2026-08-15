use super::literal::{quote_identifier, quote_string};
use crate::domain::{Column, TableKind};

pub(in crate::adapters::mysql) const TABLES_QUERY: &str = "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_ROWS, TABLE_COMMENT FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW') ORDER BY TABLE_SCHEMA, TABLE_NAME";
pub(in crate::adapters::mysql) const TABLES_RESULT_COLUMNS: &[&str] = &[
    "TABLE_SCHEMA",
    "TABLE_NAME",
    "TABLE_TYPE",
    "TABLE_ROWS",
    "TABLE_COMMENT",
];
pub(in crate::adapters::mysql) const COLUMN_METADATA_RESULT_COLUMNS: &[&str] = &[
    "COLUMN_NAME",
    "COLUMN_TYPE",
    "IS_NULLABLE",
    "COLUMN_DEFAULT",
    "EXTRA",
    "COLUMN_COMMENT",
    "ORDINAL_POSITION",
    "PRIMARY_KEY_POSITION",
];
pub(in crate::adapters::mysql) const UNIQUE_COLUMN_RESULT_COLUMNS: &[&str] = &["COLUMN_NAME"];
pub(in crate::adapters::mysql) const FOREIGN_KEY_RESULT_COLUMNS: &[&str] = &[
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
];

pub(in crate::adapters::mysql) fn table_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT t.TABLE_SCHEMA, t.TABLE_NAME, t.TABLE_TYPE, t.TABLE_ROWS, t.TABLE_COMMENT FROM INFORMATION_SCHEMA.TABLES AS t WHERE t.TABLE_SCHEMA = {} AND t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') AND t.TABLE_NAME = {} ORDER BY TABLE_SCHEMA, TABLE_NAME",
        quote_string(schema),
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) fn columns_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {} AND c.TABLE_NAME = {} ORDER BY ORDINAL_POSITION",
        quote_string(schema),
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) fn unique_columns_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT MIN(s.COLUMN_NAME) AS COLUMN_NAME FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = {} AND s.TABLE_NAME = {} AND s.NON_UNIQUE = 0 AND s.INDEX_NAME <> 'PRIMARY' GROUP BY s.INDEX_NAME HAVING COUNT(*) = 1 AND COUNT(s.COLUMN_NAME) = 1 ORDER BY s.INDEX_NAME",
        quote_string(schema),
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) fn foreign_keys_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = {} AND tc.TABLE_SCHEMA = {} AND tc.TABLE_NAME = {} AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION",
        quote_string(schema),
        quote_string(schema),
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) const SIGNATURE_COLUMNS_QUERY: &str = "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = DATABASE() ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION";
pub(in crate::adapters::mysql) const SIGNATURE_COLUMNS_RESULT_COLUMNS: &[&str] = &[
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
];
pub(in crate::adapters::mysql) const SIGNATURE_UNIQUE_COLUMNS_QUERY: &str = "SELECT s.TABLE_NAME, MIN(s.COLUMN_NAME) AS COLUMN_NAME FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = DATABASE() AND s.NON_UNIQUE = 0 AND s.INDEX_NAME <> 'PRIMARY' GROUP BY s.TABLE_NAME, s.INDEX_NAME HAVING COUNT(*) = 1 AND COUNT(s.COLUMN_NAME) = 1 ORDER BY s.TABLE_NAME, s.INDEX_NAME";
pub(in crate::adapters::mysql) const SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS: &[&str] =
    &["TABLE_NAME", "COLUMN_NAME"];
pub(in crate::adapters::mysql) const SIGNATURE_FOREIGN_KEYS_QUERY: &str = "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = DATABASE() AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ORDER BY TABLE_SCHEMA, TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION";

pub(in crate::adapters::mysql) const INDEX_RESULT_COLUMNS: &[&str] = &[
    "INDEX_NAME",
    "NON_UNIQUE",
    "INDEX_TYPE",
    "SEQ_IN_INDEX",
    "COLUMN_NAME",
    "EXPRESSION",
    "IS_PRIMARY",
];
pub(in crate::adapters::mysql) const TRIGGER_RESULT_COLUMNS: &[&str] = &[
    "TRIGGER_NAME",
    "ACTION_TIMING",
    "EVENT_MANIPULATION",
    "ACTION_STATEMENT",
    "DEFINER",
];
const TABLE_SHOW_CREATE_RESULT_COLUMNS: &[&str] = &["Table", "Create Table"];
const VIEW_SHOW_CREATE_RESULT_COLUMNS: &[&str] = &["View", "Create View"];

pub(in crate::adapters::mysql) fn indexes_query(table: &str) -> String {
    format!(
        "SELECT s.INDEX_NAME, s.NON_UNIQUE, s.INDEX_TYPE, s.SEQ_IN_INDEX, s.COLUMN_NAME, s.EXPRESSION, CASE WHEN tc.CONSTRAINT_TYPE = 'PRIMARY KEY' THEN 'YES' ELSE 'NO' END AS IS_PRIMARY FROM INFORMATION_SCHEMA.STATISTICS AS s LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = s.TABLE_SCHEMA AND tc.TABLE_SCHEMA = s.TABLE_SCHEMA AND tc.TABLE_NAME = s.TABLE_NAME AND tc.CONSTRAINT_NAME = s.INDEX_NAME WHERE s.TABLE_SCHEMA = DATABASE() AND s.TABLE_NAME = {} ORDER BY INDEX_NAME, SEQ_IN_INDEX",
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) fn triggers_query(table: &str) -> String {
    format!(
        "SELECT TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION, ACTION_STATEMENT, DEFINER FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = DATABASE() AND EVENT_OBJECT_SCHEMA = DATABASE() AND EVENT_OBJECT_TABLE = {} ORDER BY TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION",
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) fn show_create_query(table: &str, kind: TableKind) -> String {
    let object_type = if kind == TableKind::View {
        "VIEW"
    } else {
        "TABLE"
    };
    format!("SHOW CREATE {object_type} {}", quote_identifier(table))
}

pub(in crate::adapters::mysql) fn show_create_result_columns(
    kind: TableKind,
) -> &'static [&'static str] {
    if kind == TableKind::View {
        VIEW_SHOW_CREATE_RESULT_COLUMNS
    } else {
        TABLE_SHOW_CREATE_RESULT_COLUMNS
    }
}

const PREVIEW_IDENTITY_ALIAS_PREFIX: &str = "__sabiql_row_identity_";

pub(in crate::adapters::mysql) fn build_preview_query(
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

pub(in crate::adapters::mysql) fn preview_identity_alias(index: usize) -> String {
    format!("{PREVIEW_IDENTITY_ALIAS_PREFIX}{index}")
}

pub(in crate::adapters::mysql) fn build_metadata_select_query(
    query: &str,
    source_alias: &str,
    marker_alias: &str,
) -> String {
    format!(
        "WITH {source_alias} AS (SELECT * FROM (({query}\n) LIMIT 0) AS __sabiql_metadata_inner) SELECT {source_alias}.* FROM {source_alias} RIGHT JOIN (SELECT 1 AS {marker_alias}) AS __sabiql_metadata_marker ON TRUE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ColumnAttributes;

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
    fn builds_metadata_select_query_without_changing_the_fallback_sql() {
        assert_eq!(
            build_metadata_select_query("SELECT 1", "__source", "__marker"),
            "WITH __source AS (SELECT * FROM ((SELECT 1\n) LIMIT 0) AS __sabiql_metadata_inner) SELECT __source.* FROM __source RIGHT JOIN (SELECT 1 AS __marker) AS __sabiql_metadata_marker ON TRUE"
        );
    }

    #[test]
    fn metadata_queries_escape_literals_and_preserve_scope_conditions() {
        let schema = "app\\\n\r\t\u{0008}\u{001a}'";
        let table = "items\\\n\r\t\u{0008}\u{001a}'";

        assert_eq!(
            quote_string(schema),
            format!(
                "'app{}{}{}{}{}{}{}'",
                r"\\", r"\n", r"\r", r"\t", r"\b", r"\Z", r"\'",
            )
        );
        assert_eq!(
            quote_string(table),
            format!(
                "'items{}{}{}{}{}{}{}'",
                r"\\", r"\n", r"\r", r"\t", r"\b", r"\Z", r"\'",
            )
        );

        let quoted_schema = quote_string(schema);
        let quoted_table = quote_string(table);

        assert_eq!(
            table_query(schema, table),
            format!(
                "SELECT t.TABLE_SCHEMA, t.TABLE_NAME, t.TABLE_TYPE, t.TABLE_ROWS, t.TABLE_COMMENT FROM INFORMATION_SCHEMA.TABLES AS t WHERE t.TABLE_SCHEMA = {quoted_schema} AND t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') AND t.TABLE_NAME = {quoted_table} ORDER BY TABLE_SCHEMA, TABLE_NAME"
            )
        );

        assert_eq!(
            columns_query(schema, table),
            format!(
                "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {quoted_schema} AND c.TABLE_NAME = {quoted_table} ORDER BY ORDINAL_POSITION"
            )
        );

        assert_eq!(
            unique_columns_query(schema, table),
            format!(
                "SELECT MIN(s.COLUMN_NAME) AS COLUMN_NAME FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = {quoted_schema} AND s.TABLE_NAME = {quoted_table} AND s.NON_UNIQUE = 0 AND s.INDEX_NAME <> 'PRIMARY' GROUP BY s.INDEX_NAME HAVING COUNT(*) = 1 AND COUNT(s.COLUMN_NAME) = 1 ORDER BY s.INDEX_NAME"
            )
        );

        assert_eq!(
            foreign_keys_query(schema, table),
            format!(
                "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = {quoted_schema} AND tc.TABLE_SCHEMA = {quoted_schema} AND tc.TABLE_NAME = {quoted_table} AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION"
            )
        );

        assert_eq!(
            indexes_query(table),
            format!(
                "SELECT s.INDEX_NAME, s.NON_UNIQUE, s.INDEX_TYPE, s.SEQ_IN_INDEX, s.COLUMN_NAME, s.EXPRESSION, CASE WHEN tc.CONSTRAINT_TYPE = 'PRIMARY KEY' THEN 'YES' ELSE 'NO' END AS IS_PRIMARY FROM INFORMATION_SCHEMA.STATISTICS AS s LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = s.TABLE_SCHEMA AND tc.TABLE_SCHEMA = s.TABLE_SCHEMA AND tc.TABLE_NAME = s.TABLE_NAME AND tc.CONSTRAINT_NAME = s.INDEX_NAME WHERE s.TABLE_SCHEMA = DATABASE() AND s.TABLE_NAME = {quoted_table} ORDER BY INDEX_NAME, SEQ_IN_INDEX"
            )
        );

        assert_eq!(
            triggers_query(table),
            format!(
                "SELECT TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION, ACTION_STATEMENT, DEFINER FROM INFORMATION_SCHEMA.TRIGGERS WHERE TRIGGER_SCHEMA = DATABASE() AND EVENT_OBJECT_SCHEMA = DATABASE() AND EVENT_OBJECT_TABLE = {quoted_table} ORDER BY TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION"
            )
        );

        assert_eq!(
            TABLES_QUERY,
            "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_ROWS, TABLE_COMMENT FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE IN ('BASE TABLE', 'VIEW') ORDER BY TABLE_SCHEMA, TABLE_NAME"
        );
        assert_eq!(
            SIGNATURE_COLUMNS_QUERY,
            "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = DATABASE() ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION"
        );
        assert_eq!(
            SIGNATURE_UNIQUE_COLUMNS_QUERY,
            "SELECT s.TABLE_NAME, MIN(s.COLUMN_NAME) AS COLUMN_NAME FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = DATABASE() AND s.NON_UNIQUE = 0 AND s.INDEX_NAME <> 'PRIMARY' GROUP BY s.TABLE_NAME, s.INDEX_NAME HAVING COUNT(*) = 1 AND COUNT(s.COLUMN_NAME) = 1 ORDER BY s.TABLE_NAME, s.INDEX_NAME"
        );
        assert_eq!(
            SIGNATURE_FOREIGN_KEYS_QUERY,
            "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = DATABASE() AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ORDER BY TABLE_SCHEMA, TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION"
        );

        for query in [
            TABLES_QUERY,
            SIGNATURE_COLUMNS_QUERY,
            SIGNATURE_UNIQUE_COLUMNS_QUERY,
            SIGNATURE_FOREIGN_KEYS_QUERY,
            &table_query(schema, table),
            &columns_query(schema, table),
            &unique_columns_query(schema, table),
            &foreign_keys_query(schema, table),
            &indexes_query(table),
            &triggers_query(table),
        ] {
            assert!(!query.contains("UNION ALL SELECT NULL"));
        }

        assert_eq!(
            TABLES_RESULT_COLUMNS,
            &[
                "TABLE_SCHEMA",
                "TABLE_NAME",
                "TABLE_TYPE",
                "TABLE_ROWS",
                "TABLE_COMMENT"
            ]
        );
        assert_eq!(
            COLUMN_METADATA_RESULT_COLUMNS,
            &[
                "COLUMN_NAME",
                "COLUMN_TYPE",
                "IS_NULLABLE",
                "COLUMN_DEFAULT",
                "EXTRA",
                "COLUMN_COMMENT",
                "ORDINAL_POSITION",
                "PRIMARY_KEY_POSITION",
            ]
        );
        assert_eq!(UNIQUE_COLUMN_RESULT_COLUMNS, &["COLUMN_NAME"]);
        assert_eq!(
            FOREIGN_KEY_RESULT_COLUMNS,
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
            ]
        );
        assert_eq!(
            SIGNATURE_COLUMNS_RESULT_COLUMNS,
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
            ]
        );
        assert_eq!(
            SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS,
            &["TABLE_NAME", "COLUMN_NAME"]
        );
        assert_eq!(
            INDEX_RESULT_COLUMNS,
            &[
                "INDEX_NAME",
                "NON_UNIQUE",
                "INDEX_TYPE",
                "SEQ_IN_INDEX",
                "COLUMN_NAME",
                "EXPRESSION",
                "IS_PRIMARY",
            ]
        );
        assert_eq!(
            TRIGGER_RESULT_COLUMNS,
            &[
                "TRIGGER_NAME",
                "ACTION_TIMING",
                "EVENT_MANIPULATION",
                "ACTION_STATEMENT",
                "DEFINER",
            ]
        );
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
        assert_eq!(
            show_create_result_columns(TableKind::Table),
            &["Table", "Create Table"]
        );
        assert_eq!(
            show_create_result_columns(TableKind::View),
            &["View", "Create View"]
        );
    }

    #[test]
    fn preview_query_lists_visible_and_hidden_identity_columns() {
        let columns = vec![column("id", "int"), column("display", "text")];
        assert_eq!(
            build_preview_query(
                "app",
                "items",
                &["id".to_string()],
                &columns,
                &[],
                500,
                1000,
            ),
            "SELECT `id`, `display` FROM `app`.`items` ORDER BY `id` LIMIT 500 OFFSET 1000"
        );

        let visible_columns = vec![column("payload", "text")];
        let identity_columns = vec![column("id", "int")];
        assert_eq!(
            build_preview_query(
                "app",
                "items",
                &["id".to_string()],
                &visible_columns,
                &identity_columns,
                10,
                0,
            ),
            "SELECT `payload`, `id` AS `__sabiql_row_identity_0` FROM `app`.`items` ORDER BY `id` LIMIT 10 OFFSET 0"
        );
    }
}
