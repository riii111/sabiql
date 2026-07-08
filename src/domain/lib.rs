// Domain models - fields/methods defined to match DB schema

pub mod column;
pub mod command_tag;
pub mod connection;
pub mod er;
pub mod explain_plan;
pub mod foreign_key;
pub mod index;
pub mod metadata;
pub mod query_history;
pub mod query_result;
pub mod rls;
pub mod schema;
pub mod sqlite_diagnostics;
pub mod table;
pub mod table_kind;
pub mod trigger;
pub mod write_result;

pub use column::{Column, ColumnAttributes};
pub use command_tag::CommandTag;
#[cfg(test)]
pub use er::ErFkInfo;
pub use er::ErTableInfo;
pub use foreign_key::{FkAction, ForeignKey, UNRESOLVED_FK_COLUMN};
pub use index::{Index, IndexAttributes, IndexType};
pub use metadata::{DatabaseMetadata, MetadataState};
pub use query_result::{QueryResult, QuerySource, QueryValue};
pub use rls::{RlsCommand, RlsInfo, RlsPolicy};
pub use schema::Schema;
pub use sqlite_diagnostics::{DiagnosticField, SqliteDiagnosticsSnapshot};
pub use table::{Table, TableSignature, TableSummary, available_sqlite_rowid_alias};
pub use table_kind::{TableKind, TableKindInfo};
pub use trigger::{Trigger, TriggerEvent, TriggerTiming};
pub use write_result::WriteExecutionResult;

pub use connection::{
    ConnectionConfig, ConnectionId, ConnectionProfile, ConnectionProfileError, DatabaseType,
    PostgresConnectionConfig, SqliteConnectionConfig, SqliteConnectionConfigError, SqlitePathError,
    SslMode, classify_sqlite_metadata_error, classify_sqlite_read_error, sqlite_path_from_dsn,
};
