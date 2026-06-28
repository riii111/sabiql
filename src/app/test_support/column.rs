use crate::domain::{Column, ColumnAttributes};

#[must_use]
pub fn test_nullable_column(
    name: impl Into<String>,
    data_type: impl Into<String>,
    ordinal_position: i32,
) -> Column {
    Column {
        name: name.into(),
        data_type: data_type.into(),
        attributes: ColumnAttributes::NULLABLE,
        ordinal_position,
        default: None,
        comment: None,
    }
}
