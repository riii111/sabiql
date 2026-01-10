// Domain models - fields/methods defined to match DB schema
#![allow(dead_code)]

pub mod column;
pub mod connection;
pub mod er;
pub mod foreign_key;
pub mod index;
pub mod metadata;
pub mod query_result;
pub mod rls;
pub mod schema;
pub mod table;

pub use column::Column;
#[cfg(test)]
pub use er::ErFkInfo;
pub use er::ErTableInfo;
pub use foreign_key::{FkAction, ForeignKey};
pub use index::{Index, IndexType};
pub use metadata::{DatabaseMetadata, MetadataState};
pub use query_result::{QueryResult, QuerySource};
pub use rls::{RlsCommand, RlsInfo, RlsPolicy};
pub use schema::Schema;
pub use table::{Table, TableSummary};

pub use connection::{ConnectionId, ConnectionProfile, SslMode};
