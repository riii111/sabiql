//! Port traits and their error types.
//!
//! Error variants may preserve `Error::source()` chains, but method signatures
//! stay free of adapter-specific types.

pub mod access_mode;
pub mod cached_result_exporter;
pub mod clipboard;
pub mod connection_store;
pub mod db_operation_error;
pub mod er_exporter;
pub mod folder_opener;
pub mod metadata;
pub mod mysql_connection_probe;
pub mod query_executor;
pub mod query_history;
pub mod renderer;
pub mod service_file;
pub mod settings_store;
pub mod sqlite_diagnostics;
pub mod sqlite_path_validator;

pub use access_mode::AccessMode;
pub use cached_result_exporter::CachedResultExporter;
pub use clipboard::{ClipboardError, ClipboardWriter};
pub use connection_store::{ConnectionStore, ConnectionStoreError};
pub use db_operation_error::{
    ConnectionFailureKind, DatabaseCli, DbOperationError, SqliteCompatibilityKind,
    UnsupportedOperationKind,
};
pub use er_exporter::{ErDiagramExporter, ErExportError, ErExportResult};
pub use folder_opener::FolderOpener;
pub use metadata::MetadataProvider;
pub use mysql_connection_probe::{MySqlConnectionProbe, MySqlConnectionProbeResult};
pub use query_executor::QueryExecutor;
pub use query_history::{QueryHistoryError, QueryHistoryStore};
pub use renderer::{CellDetailViewport, RenderError, RenderOutput, RenderResult, Renderer};
pub use service_file::{PgServiceEntryReader, ServiceFileError};
pub use settings_store::{AppSettings, SettingsStore, SettingsStoreError};
pub use sqlite_diagnostics::SqliteDiagnosticsProvider;
pub use sqlite_path_validator::SqlitePathValidator;
