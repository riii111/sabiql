mod connection;
mod helpers;

pub use connection::reduce_connection;
pub use helpers::{
    char_count, char_to_byte_index, insert_char_at_cursor, validate_all, validate_field,
};
