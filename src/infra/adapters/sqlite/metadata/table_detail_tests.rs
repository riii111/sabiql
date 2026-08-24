use super::super::super::sqlite3::metadata::RawIndexColumn;
use super::super::SqliteAdapter;
use super::index_key_column_names;

#[path = "table_detail_columns_tests.rs"]
mod columns;
#[path = "table_detail_foreign_key_tests.rs"]
mod foreign_keys;
#[path = "table_detail_indexes_tests.rs"]
mod indexes;
#[path = "table_detail_source_ddl_tests.rs"]
mod source_ddl;
#[path = "table_detail_general_tests.rs"]
mod table_detail;
#[path = "table_detail_trigger_tests.rs"]
mod trigger_metadata;
