//! Port traits and their error types.
//!
//! Error variants carry source types (`std::io::Error`, `arboard::Error`, etc.)
//! via `#[source]` to preserve `Error::source()` chains. Method signatures stay
//! free of adapter-specific types; only error sources are exposed.

pub mod inbound;
pub mod outbound;

pub use inbound::{handle_input, InputEvent, InputKeyCombo, Key, Modifiers};
pub use outbound::{
    ClipboardError,
    ClipboardWriter,
    ConfigWriter,
    ConfigWriterError,
    ConnectionStore,
    ConnectionStoreError,
    DatabaseCapabilities,
    DatabaseCapabilityProvider,
    DbOperationError,
    DdlGenerator,
    DsnBuilder,
    ErDiagramExporter,
    ErExportError,
    ErExportResult,
    ErLogWriter,
    FolderOpenError,
    FolderOpener,
    GraphvizError,
    GraphvizRunner,
    InspectorFeature,
    MetadataProvider,
    QueryExecutor,
    QueryHistoryError,
    QueryHistoryStore,
    PgServiceEntryReader,
    RenderError,
    RenderOutput,
    RenderResult,
    Renderer,
    ServiceFileError,
    SqlDialect,
    ViewerError,
    ViewerLauncher,
};
