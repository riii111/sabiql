// Domain models - some fields/methods are defined for future use
#![allow(dead_code)]

pub mod column;
pub mod foreign_key;
pub mod graph;
pub mod index;
pub mod metadata;
pub mod query_result;
pub mod rls;
pub mod schema;
pub mod table;

pub use column::Column;
pub use foreign_key::{FkAction, ForeignKey};
pub use graph::{GraphEdge, GraphNode, NeighborhoodGraph};
pub use index::{Index, IndexType};
pub use metadata::{DatabaseMetadata, MetadataState};
pub use query_result::{QueryResult, QuerySource};
pub use rls::{RlsCommand, RlsInfo, RlsPolicy};
pub use schema::Schema;
pub use table::{Table, TableSummary};
