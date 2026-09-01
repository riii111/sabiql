mod ddl;
mod literal;
mod metadata;

pub(super) use metadata::{
    COLUMN_METADATA_BASE_RESULT_COLUMNS, EFFECTIVE_USER_QUERY, EFFECTIVE_USER_RESULT_COLUMNS,
    FOREIGN_KEY_RESULT_COLUMNS, PREVIEW_COLUMN_METADATA_RESULT_COLUMNS, SIGNATURE_COLUMNS_QUERY,
    SIGNATURE_COLUMNS_RESULT_COLUMNS, SIGNATURE_FOREIGN_KEYS_QUERY, SIGNATURE_UNIQUE_COLUMNS_QUERY,
    SIGNATURE_UNIQUE_COLUMNS_RESULT_COLUMNS, TABLES_QUERY, TABLES_RESULT_COLUMNS,
    TRIGGER_RESULT_COLUMNS, UNIQUE_COLUMN_RESULT_COLUMNS, build_legacy_metadata_select_query,
    build_metadata_select_query, build_preview_query, column_metadata_result_columns,
    columns_query_for_capabilities, foreign_keys_query, index_result_columns,
    indexes_query_for_capabilities, preview_columns_query, preview_identity_alias,
    show_create_query, show_create_result_columns, table_query, triggers_query,
    unique_columns_query,
};

#[allow(unused_imports, reason = "re-exported for MySQL metadata unit tests")]
pub(super) use metadata::{COLUMN_METADATA_RESULT_COLUMNS, INDEX_RESULT_COLUMNS};
