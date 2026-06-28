use crate::domain::{Column, ColumnAttributes};

#[must_use]
pub fn with_attributes(
    name: impl Into<String>,
    data_type: impl Into<String>,
    attributes: ColumnAttributes,
    ordinal_position: i32,
) -> Column {
    Column {
        name: name.into(),
        data_type: data_type.into(),
        attributes,
        ordinal_position,
        default: None,
        comment: None,
    }
}
