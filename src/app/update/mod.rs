pub mod action;
pub mod browse;
pub mod connection;
pub mod dispatch_result;
pub mod er;
pub mod explain;
pub mod helpers;
pub mod input;
pub mod modal;
pub mod reducer;
pub mod sql_editor;

// Facade: re-export sub-reducer entry points for update/reducer.rs dispatch
pub use browse::metadata::dispatch_metadata;
pub use browse::navigation::dispatch_navigation;
pub use browse::query::dispatch_query;
pub use browse::result::dispatch_result;
pub use connection::dispatch_connection;
pub use dispatch_result::DispatchResult;
pub use er::dispatch_er;
pub use explain::dispatch_explain;
pub use helpers::{char_to_byte_index, validate_all, validate_field};
pub use modal::dispatch_modal;
pub use sql_editor::dispatch_sql_modal;
