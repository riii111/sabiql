//! Port traits and their error types.
//!
//! Error variants carry source types (`std::io::Error`, `arboard::Error`, etc.)
//! via `#[source]` to preserve `Error::source()` chains. Method signatures stay
//! free of adapter-specific types; only error sources are exposed.
//!
//! External layers should use `inbound` / `outbound` explicitly. The flat
//! re-exports below are kept crate-local so app internals can stay concise
//! without giving adapters a polarity-blind entrypoint.

pub mod inbound;
pub mod outbound;

#[allow(unused_imports, reason = "crate-local facade for app internals")]
pub(crate) use inbound::{InputEvent, InputKeyCombo, Key, Modifiers, handle_input};
#[allow(unused_imports, reason = "crate-local facade for app internals")]
pub(crate) use outbound::{
    ClipboardError, ClipboardWriter, ConfigWriter, ConfigWriterError, ConnectionStore,
    ConnectionStoreError, DatabaseCapabilities, DatabaseCapabilityProvider, DbOperationError,
    DdlGenerator, DsnBuilder, ErDiagramExporter, ErExportError, ErExportResult, ErLogWriter,
    FolderOpenError, FolderOpener, GraphvizError, GraphvizRunner, InspectorFeature,
    MetadataProvider, PgServiceEntryReader, QueryExecutor, QueryHistoryError, QueryHistoryStore,
    RenderError, RenderOutput, RenderResult, Renderer, ServiceFileError, SqlDialect, ViewerError,
    ViewerLauncher,
};
