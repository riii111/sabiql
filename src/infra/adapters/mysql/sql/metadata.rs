use super::literal::{quote_identifier, quote_string};
use crate::domain::{Column, TableKind};

pub(in crate::adapters::mysql) const EFFECTIVE_USER_QUERY: &str = "SELECT CURRENT_USER()";
pub(in crate::adapters::mysql) const EFFECTIVE_USER_RESULT_COLUMNS: &[&str] = &["CURRENT_USER()"];
pub(in crate::adapters::mysql) const TABLES_QUERY: &str = "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_ROWS, TABLE_COMMENT, ENGINE, ROW_FORMAT, TABLE_COLLATION, CREATE_OPTIONS FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_SCHEMA, TABLE_NAME";
pub(in crate::adapters::mysql) const TABLES_RESULT_COLUMNS: &[&str] = &[
    "TABLE_SCHEMA",
    "TABLE_NAME",
    "TABLE_TYPE",
    "TABLE_ROWS",
    "TABLE_COMMENT",
    "ENGINE",
    "ROW_FORMAT",
    "TABLE_COLLATION",
    "CREATE_OPTIONS",
];
pub(in crate::adapters::mysql) const COLUMN_METADATA_BASE_RESULT_COLUMNS: &[&str] = &[
    "COLUMN_NAME",
    "COLUMN_TYPE",
    "IS_NULLABLE",
    "COLUMN_DEFAULT",
    "EXTRA",
    "COLUMN_COMMENT",
    "ORDINAL_POSITION",
    "PRIMARY_KEY_POSITION",
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
    "CHARACTER_SET_NAME",
    "COLLATION_NAME",
    "GENERATION_EXPRESSION",
];
pub(in crate::adapters::mysql) const PREVIEW_COLUMN_METADATA_RESULT_COLUMNS: &[&str] = &[
    "COLUMN_NAME",
    "COLUMN_TYPE",
    "IS_NULLABLE",
    "COLUMN_DEFAULT",
    "EXTRA",
    "COLUMN_COMMENT",
    "ORDINAL_POSITION",
    "PRIMARY_KEY_POSITION",
    "CHARACTER_SET_NAME",
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
pub(in crate::adapters::mysql) const TABLE_DETAIL_METADATA_RESULT_COLUMNS: &[&str] =
    &["METADATA_JSON"];

pub(in crate::adapters::mysql) fn table_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT t.TABLE_SCHEMA, t.TABLE_NAME, t.TABLE_TYPE, t.TABLE_ROWS, t.TABLE_COMMENT, t.ENGINE, t.ROW_FORMAT, t.TABLE_COLLATION, t.CREATE_OPTIONS FROM INFORMATION_SCHEMA.TABLES AS t WHERE t.TABLE_SCHEMA = {} AND t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') AND t.TABLE_NAME = {} ORDER BY TABLE_SCHEMA, TABLE_NAME",
        quote_string(schema),
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) fn columns_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION, c.CHARACTER_SET_NAME, c.COLLATION_NAME, c.GENERATION_EXPRESSION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {} AND c.TABLE_NAME = {} ORDER BY ORDINAL_POSITION",
        quote_string(schema),
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) fn preview_columns_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION, c.CHARACTER_SET_NAME FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {} AND c.TABLE_NAME = {} ORDER BY ORDINAL_POSITION",
        quote_string(schema),
        quote_string(table),
    )
}

pub(in crate::adapters::mysql) fn unique_columns_query(schema: &str, table: &str) -> String {
    format!(
        "SELECT MIN(s.COLUMN_NAME) AS COLUMN_NAME FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = {} AND s.TABLE_NAME = {} AND s.NON_UNIQUE = 0 AND s.INDEX_NAME <> 'PRIMARY' GROUP BY s.INDEX_NAME HAVING COUNT(*) = 1 AND COUNT(s.COLUMN_NAME) = 1 AND COUNT(s.SUB_PART) = 0 ORDER BY s.INDEX_NAME",
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

pub(in crate::adapters::mysql) fn table_detail_metadata_query(schema: &str, table: &str) -> String {
    let quoted_schema = quote_string(schema);
    let quoted_table = quote_string(table);
    format!(
        concat!(
            "SELECT metadata_rows.METADATA_JSON FROM (",
            "SELECT 0 AS METADATA_ROW_KIND, '' AS METADATA_TRIGGER_EVENT, ",
            "'' AS METADATA_TRIGGER_TIMING, 0 AS METADATA_TRIGGER_ORDER, JSON_OBJECT(",
            "'kind', 'metadata', ",
            "'tables', COALESCE((SELECT JSON_ARRAYAGG(JSON_OBJECT(",
            "'TABLE_SCHEMA', t.TABLE_SCHEMA, 'TABLE_NAME', t.TABLE_NAME, ",
            "'TABLE_TYPE', t.TABLE_TYPE, 'TABLE_ROWS', t.TABLE_ROWS, ",
            "'TABLE_COMMENT', t.TABLE_COMMENT, 'ENGINE', t.ENGINE, ",
            "'ROW_FORMAT', t.ROW_FORMAT, 'TABLE_COLLATION', t.TABLE_COLLATION, ",
            "'CREATE_OPTIONS', t.CREATE_OPTIONS)) FROM (SELECT t.TABLE_SCHEMA, ",
            "t.TABLE_NAME, t.TABLE_TYPE, t.TABLE_ROWS, t.TABLE_COMMENT, t.ENGINE, ",
            "t.ROW_FORMAT, t.TABLE_COLLATION, t.CREATE_OPTIONS FROM ",
            "INFORMATION_SCHEMA.TABLES AS t WHERE t.TABLE_SCHEMA = {schema} AND ",
            "t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') AND t.TABLE_NAME = {table} ",
            ") AS t), JSON_ARRAY()), ",
            "'columns', COALESCE((SELECT JSON_ARRAYAGG(JSON_OBJECT(",
            "'COLUMN_NAME', c.COLUMN_NAME, 'COLUMN_TYPE', c.COLUMN_TYPE, ",
            "'IS_NULLABLE', c.IS_NULLABLE, 'COLUMN_DEFAULT', c.COLUMN_DEFAULT, ",
            "'EXTRA', c.EXTRA, 'COLUMN_COMMENT', c.COLUMN_COMMENT, ",
            "'ORDINAL_POSITION', c.ORDINAL_POSITION, ",
            "'PRIMARY_KEY_POSITION', c.PRIMARY_KEY_POSITION, ",
            "'CHARACTER_SET_NAME', c.CHARACTER_SET_NAME, ",
            "'COLLATION_NAME', c.COLLATION_NAME, ",
            "'GENERATION_EXPRESSION', c.GENERATION_EXPRESSION)) FROM ",
            "(SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, ",
            "c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, ",
            "kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION, c.CHARACTER_SET_NAME, ",
            "c.COLLATION_NAME, c.GENERATION_EXPRESSION FROM ",
            "INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN ",
            "INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON ",
            "tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA ",
            "AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' ",
            "AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN ",
            "INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON ",
            "kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND ",
            "kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME ",
            "AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND ",
            "kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {schema} ",
            "AND c.TABLE_NAME = {table}) AS c), JSON_ARRAY()), ",
            "'statistics', COALESCE((SELECT JSON_ARRAYAGG(JSON_OBJECT(",
            "'INDEX_NAME', s.INDEX_NAME, 'NON_UNIQUE', s.NON_UNIQUE, ",
            "'INDEX_TYPE', s.INDEX_TYPE, 'SEQ_IN_INDEX', s.SEQ_IN_INDEX, ",
            "'COLUMN_NAME', s.COLUMN_NAME, 'SUB_PART', s.SUB_PART, ",
            "'EXPRESSION', s.EXPRESSION, 'COLLATION', s.COLLATION, ",
            "'IS_VISIBLE', s.IS_VISIBLE, 'IS_PRIMARY', s.IS_PRIMARY)) FROM ",
            "(SELECT s.INDEX_NAME, s.NON_UNIQUE, s.INDEX_TYPE, s.SEQ_IN_INDEX, ",
            "s.COLUMN_NAME, s.SUB_PART, s.EXPRESSION, s.COLLATION, s.IS_VISIBLE, ",
            "CASE WHEN s.INDEX_NAME = 'PRIMARY' THEN 'YES' ELSE 'NO' END AS IS_PRIMARY ",
            "FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = {schema} ",
            "AND s.TABLE_NAME = {table}) AS s), ",
            "JSON_ARRAY()), 'foreign_keys', COALESCE((SELECT JSON_ARRAYAGG(JSON_OBJECT(",
            "'CONSTRAINT_NAME', fk.CONSTRAINT_NAME, 'TABLE_SCHEMA', fk.TABLE_SCHEMA, ",
            "'TABLE_NAME', fk.TABLE_NAME, 'COLUMN_NAME', fk.COLUMN_NAME, ",
            "'REFERENCED_TABLE_SCHEMA', fk.REFERENCED_TABLE_SCHEMA, ",
            "'REFERENCED_TABLE_NAME', fk.REFERENCED_TABLE_NAME, ",
            "'REFERENCED_COLUMN_NAME', fk.REFERENCED_COLUMN_NAME, ",
            "'ORDINAL_POSITION', fk.ORDINAL_POSITION, 'UPDATE_RULE', fk.UPDATE_RULE, ",
            "'DELETE_RULE', fk.DELETE_RULE)) FROM (SELECT kcu.CONSTRAINT_NAME, ",
            "kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, ",
            "kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, ",
            "kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, ",
            "rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN ",
            "INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON ",
            "kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND ",
            "kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME ",
            "AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN ",
            "INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON ",
            "rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME ",
            "AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE ",
            "tc.CONSTRAINT_SCHEMA = {schema} AND tc.TABLE_SCHEMA = {schema} ",
            "AND tc.TABLE_NAME = {table} AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ",
            ") AS fk), JSON_ARRAY())",
            ") AS METADATA_JSON ",
            "UNION ALL ",
            "SELECT 1 AS METADATA_ROW_KIND, tr.EVENT_MANIPULATION AS METADATA_TRIGGER_EVENT, ",
            "tr.ACTION_TIMING AS METADATA_TRIGGER_TIMING, tr.ACTION_ORDER AS METADATA_TRIGGER_ORDER, JSON_OBJECT(",
            "'kind', 'trigger', ",
            "'TRIGGER_NAME', tr.TRIGGER_NAME, 'ACTION_ORDER', tr.ACTION_ORDER, ",
            "'ACTION_TIMING', tr.ACTION_TIMING, ",
            "'EVENT_MANIPULATION', tr.EVENT_MANIPULATION, ",
            "'ACTION_STATEMENT', tr.ACTION_STATEMENT, 'DEFINER', tr.DEFINER, ",
            "'SQL_MODE', tr.SQL_MODE, 'CHARACTER_SET_CLIENT', tr.CHARACTER_SET_CLIENT, ",
            "'COLLATION_CONNECTION', tr.COLLATION_CONNECTION, ",
            "'DATABASE_COLLATION', tr.DATABASE_COLLATION, 'CREATED', tr.CREATED) ",
            "AS METADATA_JSON FROM INFORMATION_SCHEMA.TRIGGERS AS tr WHERE ",
            "tr.TRIGGER_SCHEMA = {schema} AND tr.EVENT_OBJECT_SCHEMA = {schema} ",
            "AND tr.EVENT_OBJECT_TABLE = {table}",
            ") AS metadata_rows ORDER BY METADATA_ROW_KIND, METADATA_TRIGGER_EVENT, ",
            "METADATA_TRIGGER_TIMING, METADATA_TRIGGER_ORDER"
        ),
        schema = quoted_schema,
        table = quoted_table,
    )
}

pub(in crate::adapters::mysql) const SIGNATURE_COLUMNS_QUERY: &str = "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = DATABASE() AND c.TABLE_NAME IN (SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE') ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION";
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
pub(in crate::adapters::mysql) const SIGNATURE_UNIQUE_COLUMNS_QUERY: &str = "SELECT s.TABLE_NAME, MIN(s.COLUMN_NAME) AS COLUMN_NAME FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = DATABASE() AND s.NON_UNIQUE = 0 AND s.INDEX_NAME <> 'PRIMARY' GROUP BY s.TABLE_NAME, s.INDEX_NAME HAVING COUNT(*) = 1 AND COUNT(s.COLUMN_NAME) = 1 AND COUNT(s.SUB_PART) = 0 ORDER BY s.TABLE_NAME, s.INDEX_NAME";
pub(in crate::adapters::mysql) const SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS: &[&str] =
    &["TABLE_NAME", "COLUMN_NAME"];
pub(in crate::adapters::mysql) const SIGNATURE_FOREIGN_KEYS_QUERY: &str = "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = DATABASE() AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ORDER BY TABLE_SCHEMA, TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION";

pub(in crate::adapters::mysql) const INDEX_RESULT_COLUMNS: &[&str] = &[
    "INDEX_NAME",
    "NON_UNIQUE",
    "INDEX_TYPE",
    "SEQ_IN_INDEX",
    "COLUMN_NAME",
    "SUB_PART",
    "EXPRESSION",
    "COLLATION",
    "IS_VISIBLE",
    "IS_PRIMARY",
];
pub(in crate::adapters::mysql) const TRIGGER_RESULT_COLUMNS: &[&str] = &[
    "TRIGGER_NAME",
    "ACTION_ORDER",
    "ACTION_TIMING",
    "EVENT_MANIPULATION",
    "ACTION_STATEMENT",
    "DEFINER",
    "SQL_MODE",
    "CHARACTER_SET_CLIENT",
    "COLLATION_CONNECTION",
    "DATABASE_COLLATION",
    "CREATED",
];
const TABLE_SHOW_CREATE_RESULT_COLUMNS: &[&str] = &["Table", "Create Table"];
const VIEW_SHOW_CREATE_RESULT_COLUMNS: &[&str] = &["View", "Create View"];

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
    let order_clause = if order_by.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {order_by}")
    };
    format!(
        "SELECT {columns} FROM {}.{}{order_clause} LIMIT {limit} OFFSET {offset}",
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
            character_set_name: None,
            collation_name: None,
            generation_expression: None,
            generation_kind: None,
        }
    }

    #[test]
    fn metadata_queries_escape_literals_and_preserve_scope_conditions() {
        let schema = "app\\\n\r\t\u{0008}\u{001a}'";
        let table = "items\\\n\r\t\u{0008}\u{001a}'";

        assert_eq!(EFFECTIVE_USER_QUERY, "SELECT CURRENT_USER()");
        assert_eq!(EFFECTIVE_USER_RESULT_COLUMNS, &["CURRENT_USER()"][..]);

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

        let table_sql = table_query(schema, table);
        assert_eq!(
            table_sql,
            format!(
                "SELECT t.TABLE_SCHEMA, t.TABLE_NAME, t.TABLE_TYPE, t.TABLE_ROWS, t.TABLE_COMMENT, t.ENGINE, t.ROW_FORMAT, t.TABLE_COLLATION, t.CREATE_OPTIONS FROM INFORMATION_SCHEMA.TABLES AS t WHERE t.TABLE_SCHEMA = {quoted_schema} AND t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') AND t.TABLE_NAME = {quoted_table} ORDER BY TABLE_SCHEMA, TABLE_NAME"
            )
        );

        let columns_sql = columns_query(schema, table);
        assert_eq!(
            columns_sql,
            format!(
                "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION, c.CHARACTER_SET_NAME, c.COLLATION_NAME, c.GENERATION_EXPRESSION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {quoted_schema} AND c.TABLE_NAME = {quoted_table} ORDER BY ORDINAL_POSITION"
            )
        );

        let preview_columns_sql = preview_columns_query(schema, table);
        assert_eq!(
            preview_columns_sql,
            format!(
                "SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION, c.CHARACTER_SET_NAME FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {quoted_schema} AND c.TABLE_NAME = {quoted_table} ORDER BY ORDINAL_POSITION"
            )
        );

        let unique_columns_sql = unique_columns_query(schema, table);
        assert_eq!(
            unique_columns_sql,
            format!(
                "SELECT MIN(s.COLUMN_NAME) AS COLUMN_NAME FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = {quoted_schema} AND s.TABLE_NAME = {quoted_table} AND s.NON_UNIQUE = 0 AND s.INDEX_NAME <> 'PRIMARY' GROUP BY s.INDEX_NAME HAVING COUNT(*) = 1 AND COUNT(s.COLUMN_NAME) = 1 AND COUNT(s.SUB_PART) = 0 ORDER BY s.INDEX_NAME"
            )
        );

        let foreign_keys_sql = foreign_keys_query(schema, table);
        assert_eq!(
            foreign_keys_sql,
            format!(
                "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = {quoted_schema} AND tc.TABLE_SCHEMA = {quoted_schema} AND tc.TABLE_NAME = {quoted_table} AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION"
            )
        );

        let table_detail_metadata_sql = table_detail_metadata_query(schema, table);
        let expected_table_detail_metadata_sql = format!(
            concat!(
                "SELECT metadata_rows.METADATA_JSON FROM (",
                "SELECT 0 AS METADATA_ROW_KIND, '' AS METADATA_TRIGGER_EVENT, ",
                "'' AS METADATA_TRIGGER_TIMING, 0 AS METADATA_TRIGGER_ORDER, JSON_OBJECT(",
                "'kind', 'metadata', ",
                "'tables', COALESCE((SELECT JSON_ARRAYAGG(JSON_OBJECT(",
                "'TABLE_SCHEMA', t.TABLE_SCHEMA, 'TABLE_NAME', t.TABLE_NAME, ",
                "'TABLE_TYPE', t.TABLE_TYPE, 'TABLE_ROWS', t.TABLE_ROWS, ",
                "'TABLE_COMMENT', t.TABLE_COMMENT, 'ENGINE', t.ENGINE, ",
                "'ROW_FORMAT', t.ROW_FORMAT, 'TABLE_COLLATION', t.TABLE_COLLATION, ",
                "'CREATE_OPTIONS', t.CREATE_OPTIONS)) FROM (SELECT t.TABLE_SCHEMA, ",
                "t.TABLE_NAME, t.TABLE_TYPE, t.TABLE_ROWS, t.TABLE_COMMENT, t.ENGINE, ",
                "t.ROW_FORMAT, t.TABLE_COLLATION, t.CREATE_OPTIONS FROM ",
                "INFORMATION_SCHEMA.TABLES AS t WHERE t.TABLE_SCHEMA = {schema} AND ",
                "t.TABLE_TYPE IN ('BASE TABLE', 'VIEW') AND t.TABLE_NAME = {table} ",
                ") AS t), JSON_ARRAY()), ",
                "'columns', COALESCE((SELECT JSON_ARRAYAGG(JSON_OBJECT(",
                "'COLUMN_NAME', c.COLUMN_NAME, 'COLUMN_TYPE', c.COLUMN_TYPE, ",
                "'IS_NULLABLE', c.IS_NULLABLE, 'COLUMN_DEFAULT', c.COLUMN_DEFAULT, ",
                "'EXTRA', c.EXTRA, 'COLUMN_COMMENT', c.COLUMN_COMMENT, ",
                "'ORDINAL_POSITION', c.ORDINAL_POSITION, ",
                "'PRIMARY_KEY_POSITION', c.PRIMARY_KEY_POSITION, ",
                "'CHARACTER_SET_NAME', c.CHARACTER_SET_NAME, ",
                "'COLLATION_NAME', c.COLLATION_NAME, ",
                "'GENERATION_EXPRESSION', c.GENERATION_EXPRESSION)) FROM ",
                "(SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, ",
                "c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, ",
                "kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION, c.CHARACTER_SET_NAME, ",
                "c.COLLATION_NAME, c.GENERATION_EXPRESSION FROM ",
                "INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN ",
                "INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON ",
                "tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA ",
                "AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' ",
                "AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN ",
                "INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON ",
                "kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND ",
                "kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME ",
                "AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND ",
                "kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = {schema} ",
                "AND c.TABLE_NAME = {table}) AS c), JSON_ARRAY()), ",
                "'statistics', COALESCE((SELECT JSON_ARRAYAGG(JSON_OBJECT(",
                "'INDEX_NAME', s.INDEX_NAME, 'NON_UNIQUE', s.NON_UNIQUE, ",
                "'INDEX_TYPE', s.INDEX_TYPE, 'SEQ_IN_INDEX', s.SEQ_IN_INDEX, ",
                "'COLUMN_NAME', s.COLUMN_NAME, 'SUB_PART', s.SUB_PART, ",
                "'EXPRESSION', s.EXPRESSION, 'COLLATION', s.COLLATION, ",
                "'IS_VISIBLE', s.IS_VISIBLE, 'IS_PRIMARY', s.IS_PRIMARY)) FROM ",
                "(SELECT s.INDEX_NAME, s.NON_UNIQUE, s.INDEX_TYPE, s.SEQ_IN_INDEX, ",
                "s.COLUMN_NAME, s.SUB_PART, s.EXPRESSION, s.COLLATION, s.IS_VISIBLE, ",
                "CASE WHEN s.INDEX_NAME = 'PRIMARY' THEN 'YES' ELSE 'NO' END AS IS_PRIMARY ",
                "FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = {schema} ",
                "AND s.TABLE_NAME = {table}) AS s), ",
                "JSON_ARRAY()), 'foreign_keys', COALESCE((SELECT JSON_ARRAYAGG(JSON_OBJECT(",
                "'CONSTRAINT_NAME', fk.CONSTRAINT_NAME, 'TABLE_SCHEMA', fk.TABLE_SCHEMA, ",
                "'TABLE_NAME', fk.TABLE_NAME, 'COLUMN_NAME', fk.COLUMN_NAME, ",
                "'REFERENCED_TABLE_SCHEMA', fk.REFERENCED_TABLE_SCHEMA, ",
                "'REFERENCED_TABLE_NAME', fk.REFERENCED_TABLE_NAME, ",
                "'REFERENCED_COLUMN_NAME', fk.REFERENCED_COLUMN_NAME, ",
                "'ORDINAL_POSITION', fk.ORDINAL_POSITION, 'UPDATE_RULE', fk.UPDATE_RULE, ",
                "'DELETE_RULE', fk.DELETE_RULE)) FROM (SELECT kcu.CONSTRAINT_NAME, ",
                "kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, ",
                "kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, ",
                "kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, ",
                "rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN ",
                "INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON ",
                "kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND ",
                "kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME ",
                "AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN ",
                "INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON ",
                "rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME ",
                "AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE ",
                "tc.CONSTRAINT_SCHEMA = {schema} AND tc.TABLE_SCHEMA = {schema} ",
                "AND tc.TABLE_NAME = {table} AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ",
                ") AS fk), JSON_ARRAY())",
                ") AS METADATA_JSON ",
                "UNION ALL ",
                "SELECT 1 AS METADATA_ROW_KIND, tr.EVENT_MANIPULATION AS METADATA_TRIGGER_EVENT, ",
                "tr.ACTION_TIMING AS METADATA_TRIGGER_TIMING, tr.ACTION_ORDER AS METADATA_TRIGGER_ORDER, JSON_OBJECT(",
                "'kind', 'trigger', ",
                "'TRIGGER_NAME', tr.TRIGGER_NAME, 'ACTION_ORDER', tr.ACTION_ORDER, ",
                "'ACTION_TIMING', tr.ACTION_TIMING, ",
                "'EVENT_MANIPULATION', tr.EVENT_MANIPULATION, ",
                "'ACTION_STATEMENT', tr.ACTION_STATEMENT, 'DEFINER', tr.DEFINER, ",
                "'SQL_MODE', tr.SQL_MODE, 'CHARACTER_SET_CLIENT', tr.CHARACTER_SET_CLIENT, ",
                "'COLLATION_CONNECTION', tr.COLLATION_CONNECTION, ",
                "'DATABASE_COLLATION', tr.DATABASE_COLLATION, 'CREATED', tr.CREATED) ",
                "AS METADATA_JSON FROM INFORMATION_SCHEMA.TRIGGERS AS tr WHERE ",
                "tr.TRIGGER_SCHEMA = {schema} AND tr.EVENT_OBJECT_SCHEMA = {schema} ",
                "AND tr.EVENT_OBJECT_TABLE = {table}",
                ") AS metadata_rows ORDER BY METADATA_ROW_KIND, METADATA_TRIGGER_EVENT, ",
                "METADATA_TRIGGER_TIMING, METADATA_TRIGGER_ORDER"
            ),
            schema = quoted_schema,
            table = quoted_table,
        );
        assert_eq!(
            table_detail_metadata_sql,
            expected_table_detail_metadata_sql
        );

        assert_eq!(
            TABLES_QUERY,
            "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_ROWS, TABLE_COMMENT, ENGINE, ROW_FORMAT, TABLE_COLLATION, CREATE_OPTIONS FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_SCHEMA, TABLE_NAME"
        );
        assert_eq!(
            SIGNATURE_COLUMNS_QUERY,
            "SELECT c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_DEFAULT, c.EXTRA, c.COLUMN_COMMENT, c.ORDINAL_POSITION, kcu.ORDINAL_POSITION AS PRIMARY_KEY_POSITION FROM INFORMATION_SCHEMA.COLUMNS AS c LEFT JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc ON tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = c.TABLE_SCHEMA AND tc.TABLE_NAME = c.TABLE_NAME AND tc.CONSTRAINT_NAME = 'PRIMARY' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' LEFT JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME AND kcu.COLUMN_NAME = c.COLUMN_NAME WHERE c.TABLE_SCHEMA = DATABASE() AND c.TABLE_NAME IN (SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE') ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION"
        );
        assert_eq!(
            SIGNATURE_UNIQUE_COLUMNS_QUERY,
            "SELECT s.TABLE_NAME, MIN(s.COLUMN_NAME) AS COLUMN_NAME FROM INFORMATION_SCHEMA.STATISTICS AS s WHERE s.TABLE_SCHEMA = DATABASE() AND s.NON_UNIQUE = 0 AND s.INDEX_NAME <> 'PRIMARY' GROUP BY s.TABLE_NAME, s.INDEX_NAME HAVING COUNT(*) = 1 AND COUNT(s.COLUMN_NAME) = 1 AND COUNT(s.SUB_PART) = 0 ORDER BY s.TABLE_NAME, s.INDEX_NAME"
        );
        assert_eq!(
            SIGNATURE_FOREIGN_KEYS_QUERY,
            "SELECT kcu.CONSTRAINT_NAME, kcu.TABLE_SCHEMA, kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.ORDINAL_POSITION, rc.UPDATE_RULE, rc.DELETE_RULE FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS AS tc INNER JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE AS kcu ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA AND kcu.TABLE_NAME = tc.TABLE_NAME AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME INNER JOIN INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS AS rc ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.TABLE_NAME = tc.TABLE_NAME AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_SCHEMA = DATABASE() AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' ORDER BY TABLE_SCHEMA, TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION"
        );

        assert_eq!(
            TABLES_RESULT_COLUMNS,
            &[
                "TABLE_SCHEMA",
                "TABLE_NAME",
                "TABLE_TYPE",
                "TABLE_ROWS",
                "TABLE_COMMENT",
                "ENGINE",
                "ROW_FORMAT",
                "TABLE_COLLATION",
                "CREATE_OPTIONS"
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
                "CHARACTER_SET_NAME",
                "COLLATION_NAME",
                "GENERATION_EXPRESSION",
            ]
        );
        assert_eq!(
            PREVIEW_COLUMN_METADATA_RESULT_COLUMNS,
            &[
                "COLUMN_NAME",
                "COLUMN_TYPE",
                "IS_NULLABLE",
                "COLUMN_DEFAULT",
                "EXTRA",
                "COLUMN_COMMENT",
                "ORDINAL_POSITION",
                "PRIMARY_KEY_POSITION",
                "CHARACTER_SET_NAME",
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
        assert_eq!(TABLE_DETAIL_METADATA_RESULT_COLUMNS, &["METADATA_JSON"]);
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
                "SUB_PART",
                "EXPRESSION",
                "COLLATION",
                "IS_VISIBLE",
                "IS_PRIMARY",
            ]
        );
        assert_eq!(
            TRIGGER_RESULT_COLUMNS,
            &[
                "TRIGGER_NAME",
                "ACTION_ORDER",
                "ACTION_TIMING",
                "EVENT_MANIPULATION",
                "ACTION_STATEMENT",
                "DEFINER",
                "SQL_MODE",
                "CHARACTER_SET_CLIENT",
                "COLLATION_CONNECTION",
                "DATABASE_COLLATION",
                "CREATED",
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

        assert_eq!(
            build_preview_query("app", "items", &[], &visible_columns, &[], 10, 0,),
            "SELECT `payload` FROM `app`.`items` LIMIT 10 OFFSET 0"
        );
    }
}
