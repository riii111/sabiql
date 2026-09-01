mod adapter;
mod capability;
mod cli;
mod connection;
mod dsn;
mod executor;
mod metadata;
mod option_file;
mod sql;

#[cfg(feature = "test-support")]
pub mod test_support;

pub use adapter::MySqlAdapter;

#[cfg(test)]
mod metadata_test_support {
    use crate::domain::{Column, ColumnAttributes};

    pub(super) fn column(name: &str, data_type: &str) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            default: None,
            attributes: ColumnAttributes::empty(),
            comment: None,
            ordinal_position: 1,
            character_set_name: None,
            collation_name: None,
            generation_expression: None,
            generation_kind: None,
        }
    }
}
