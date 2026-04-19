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
pub mod table;
pub mod trigger;
pub mod write_result;

pub mod domain {
    pub use crate::column;
    pub use crate::command_tag;
    pub use crate::connection;
    pub use crate::er;
    pub use crate::explain_plan;
    pub use crate::foreign_key;
    pub use crate::index;
    pub use crate::metadata;
    pub use crate::query_history;
    pub use crate::query_result;
    pub use crate::rls;
    pub use crate::schema;
    pub use crate::table;
    pub use crate::trigger;
    pub use crate::write_result;
    pub use crate::{
        Column, CommandTag, ConnectionId, ConnectionProfile, DatabaseMetadata, ErTableInfo,
        FkAction, ForeignKey, Index, IndexType, MetadataState, QueryResult, QuerySource,
        RlsCommand, RlsInfo, RlsPolicy, Schema, SslMode, Table, TableSignature, TableSummary,
        Trigger, TriggerEvent, TriggerTiming, WriteExecutionResult,
    };
}

pub use column::Column;
pub use command_tag::CommandTag;
#[cfg(test)]
pub use er::ErFkInfo;
pub use er::ErTableInfo;
pub use foreign_key::{FkAction, ForeignKey};
pub use index::{Index, IndexType};
pub use metadata::{DatabaseMetadata, MetadataState};
pub use query_result::{QueryResult, QuerySource};
pub use rls::{RlsCommand, RlsInfo, RlsPolicy};
pub use schema::Schema;
pub use table::{Table, TableSignature, TableSummary};
pub use trigger::{Trigger, TriggerEvent, TriggerTiming};
pub use write_result::WriteExecutionResult;

pub use connection::{ConnectionId, ConnectionProfile, SslMode};
