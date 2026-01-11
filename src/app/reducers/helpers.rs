//! Shared helper functions for sub-reducers.

use crate::app::connection_setup_state::{ConnectionField, ConnectionSetupState};

pub fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

pub fn insert_char_at_cursor(s: &mut String, char_pos: usize, c: char) {
    let byte_idx = char_to_byte_index(s, char_pos);
    s.insert(byte_idx, c);
}

pub fn validate_field(state: &mut ConnectionSetupState, field: ConnectionField) {
    state.validation_errors.remove(&field);

    match field {
        ConnectionField::Host => {
            if state.host.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::Port => {
            if state.port.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            } else {
                match state.port.parse::<u16>() {
                    Err(_) => {
                        state
                            .validation_errors
                            .insert(field, "Invalid port".to_string());
                    }
                    Ok(0) => {
                        state
                            .validation_errors
                            .insert(field, "Port must be > 0".to_string());
                    }
                    Ok(_) => {}
                }
            }
        }
        ConnectionField::Database => {
            if state.database.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::User => {
            if state.user.trim().is_empty() {
                state
                    .validation_errors
                    .insert(field, "Required".to_string());
            }
        }
        ConnectionField::Password | ConnectionField::SslMode => {
            // Optional fields, no validation needed
        }
    }
}

pub fn validate_all(state: &mut ConnectionSetupState) {
    for field in ConnectionField::all() {
        validate_field(state, *field);
    }
}
