pub mod action;
pub(crate) mod browse;
pub(crate) mod connection;
pub(crate) mod dispatch_result;
pub(crate) mod er;
pub(crate) mod explain;
pub(crate) mod helpers;
pub mod input;
pub(crate) mod modal;
mod query_context;
pub mod reducer;
pub(crate) mod sql_editor;
#[cfg(test)]
pub(crate) mod test_fixtures;
// Facade: re-export sub-reducer entry points for update/reducer.rs dispatch
pub(crate) use browse::metadata::dispatch_metadata;
pub(crate) use browse::navigation::dispatch_navigation;
pub(crate) use browse::query::dispatch_query;
pub use browse::result::dispatch_result;
pub(crate) use connection::dispatch_connection;
pub(crate) use er::dispatch_er;
pub(crate) use explain::dispatch_explain;
pub(crate) use modal::dispatch_modal;
pub(crate) use sql_editor::dispatch_sql_modal;
