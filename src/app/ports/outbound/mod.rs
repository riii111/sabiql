//! Port traits and their error types.
//!
//! Error variants may preserve `Error::source()` chains, but method signatures
//! stay free of adapter-specific types.

pub mod clipboard;
pub mod config_writer;
pub mod connection_store;
pub mod db_operation_error;
pub mod ddl_generator;
pub mod dsn_builder;
pub mod er_exporter;
pub mod er_log_writer;
pub mod folder_opener;
pub mod metadata;
pub mod query_executor;
pub mod query_history;
pub mod renderer;
pub mod service_file;
pub mod settings_store;
pub mod sql_dialect;
pub mod sqlite_diagnostics;
pub mod sqlite_path_validator;

pub use clipboard::{ClipboardError, ClipboardWriter};
pub use config_writer::{ConfigWriter, ConfigWriterError};
pub use connection_store::{ConnectionStore, ConnectionStoreError};
pub use db_operation_error::DbOperationError;
pub use ddl_generator::DdlGenerator;
pub use dsn_builder::DsnBuilder;
pub use er_exporter::{ErDiagramExporter, ErExportError, ErExportResult};
pub use er_log_writer::ErLogWriter;
pub use folder_opener::{FolderOpenError, FolderOpener};
pub use metadata::MetadataProvider;
pub use query_executor::QueryExecutor;
pub use query_history::{QueryHistoryError, QueryHistoryStore};
pub use renderer::{CellDetailViewport, RenderError, RenderOutput, RenderResult, Renderer};
pub use service_file::{PgServiceEntryReader, ServiceFileError};
pub use settings_store::{AppSettings, SettingsStore, SettingsStoreError};
pub use sql_dialect::SqlDialect;
pub use sqlite_diagnostics::SqliteDiagnosticsProvider;
pub use sqlite_path_validator::SqlitePathValidator;
