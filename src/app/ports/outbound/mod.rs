//! Port traits and their error types.
//!
//! Error variants may preserve `Error::source()` chains, but method signatures
//! stay free of adapter-specific types.

pub mod access_mode;
pub mod cached_result_exporter;
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
pub mod mysql_connection_probe;
pub mod query_executor;
pub mod query_history;
pub mod renderer;
pub mod secret_store;
pub mod service_file;
pub mod settings_store;
pub mod sqlite_diagnostics;
pub mod sqlite_path_validator;

pub use access_mode::AccessMode;
pub use cached_result_exporter::CachedResultExporter;
pub use clipboard::{ClipboardError, ClipboardOutcome, ClipboardWriter};
pub use config_writer::{ConfigWriter, ConfigWriterError};
pub use connection_store::{ConnectionStore, ConnectionStoreError};
pub use db_operation_error::{
    ConnectionFailureKind, DatabaseCli, DbOperationError, SqliteCompatibilityKind,
    UnsupportedOperationKind,
};
pub use ddl_generator::DdlGenerator;
pub use dsn_builder::DsnBuilder;
pub use er_exporter::{ErDiagramExporter, ErExportError, ErExportResult};
pub use er_log_writer::ErLogWriter;
pub use folder_opener::FolderOpener;
pub use metadata::{MetadataFetchResult, MetadataProvider};
pub use mysql_connection_probe::{MySqlConnectionProbe, MySqlConnectionProbeResult};
pub use query_executor::QueryExecutor;
pub use query_history::{QueryHistoryError, QueryHistoryStore};
pub use renderer::{CellDetailViewport, RenderError, RenderOutput, RenderResult, Renderer};
pub use secret_store::{SecretStore, SecretStoreError};
pub use service_file::{PgServiceEntryReader, ServiceFileContents, ServiceFileError};
pub use settings_store::{AppSettings, SettingsStore, SettingsStoreError};
pub use sqlite_diagnostics::SqliteDiagnosticsProvider;
pub use sqlite_path_validator::SqlitePathValidator;
