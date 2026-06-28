use crate::domain::{Column, ColumnAttributes};

fn default_column() -> Column {
    Column {
        name: String::new(),
        data_type: String::new(),
        attributes: ColumnAttributes::NULLABLE,
        ordinal_position: 0,
        default: None,
        comment: None,
    }
}

#[must_use]
pub fn column_fixture(configure: impl FnOnce(&mut Column)) -> Column {
    let mut column = default_column();
    configure(&mut column);
    column
}
