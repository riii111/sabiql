pub mod action;
pub mod browse;
pub mod connection;
pub mod er;
pub mod explain;
pub mod helpers;
pub mod input;
pub mod modal;
pub mod reducer;
pub mod sql_editor;

// Facade: re-export sub-reducer entry points for update/reducer.rs dispatch
pub use browse::metadata::reduce_metadata;
pub use browse::navigation::reduce_navigation;
pub use browse::query::reduce_query;
pub use browse::result::reduce_result;
pub use connection::reduce_connection;
pub use er::reduce_er;
pub use explain::reduce_explain;
pub use helpers::{
    char_count, char_to_byte_index, insert_char_at_cursor, insert_str_at_cursor, validate_all,
    validate_field,
};
pub use modal::reduce_modal;
pub use sql_editor::reduce_sql_modal;
