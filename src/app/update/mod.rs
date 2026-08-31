pub mod action;
pub(in crate::update) mod browse;
pub(in crate::update) mod connection;
pub(crate) mod dispatch_result;
pub(in crate::update) mod er;
pub(in crate::update) mod explain;
pub(in crate::update) mod helpers;
pub mod input;
pub(in crate::update) mod modal;
mod query_context;
pub mod reducer;
pub(in crate::update) mod sql_editor;
#[cfg(test)]
pub(crate) mod test_fixtures;
// Facade: re-export sub-reducer entry points for update/reducer.rs dispatch
pub(crate) use browse::metadata::dispatch_metadata;
pub(in crate::update) use browse::navigation::dispatch_navigation;
pub(in crate::update) use browse::query::dispatch_query;
pub use browse::result::dispatch_result;
pub(in crate::update) use connection::dispatch_connection;
pub(in crate::update) use er::dispatch_er;
pub(in crate::update) use explain::dispatch_explain;
pub(in crate::update) use modal::dispatch_modal;
pub(in crate::update) use sql_editor::dispatch_sql_modal;
