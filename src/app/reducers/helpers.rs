//! Shared helper functions for sub-reducers.

use std::sync::Arc;

use crate::app::connection_setup_state::{ConnectionField, ConnectionSetupState};
use crate::app::state::AppState;
use crate::domain::{QueryResult, QuerySource};

/// Shared prerequisites for preview-cell write operations.
/// Entry checks in navigation and submit-time checks in query should both use this.
pub fn editable_preview_base(state: &AppState) -> Result<(Arc<QueryResult>, Vec<String>), String> {
    if state.query.history_index.is_some() {
        return Err("Editing is unavailable while browsing history".to_string());
    }

    let result = state
        .query
        .current_result
        .as_ref()
        .cloned()
        .ok_or_else(|| "No result to edit".to_string())?;
    if result.source != QuerySource::Preview || result.is_error() {
        return Err("Only Preview results are editable".to_string());
    }

    if state.query.pagination.schema.is_empty() || state.query.pagination.table.is_empty() {
        return Err("Preview target table is unknown".to_string());
    }

    let table_detail = state
        .cache
        .table_detail
        .as_ref()
        .ok_or_else(|| "Table metadata not loaded".to_string())?;

    if table_detail.schema != state.query.pagination.schema
        || table_detail.name != state.query.pagination.table
    {
        return Err("Table metadata does not match current preview target".to_string());
    }

    let pk_cols = table_detail
        .primary_key
        .as_ref()
        .filter(|cols| !cols.is_empty())
        .cloned()
        .ok_or_else(|| "Editing requires a PRIMARY KEY".to_string())?;

    Ok((result, pk_cols))
}

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

pub fn insert_str_at_cursor(s: &mut String, char_pos: usize, text: &str) -> usize {
    let byte_idx = char_to_byte_index(s, char_pos);
    s.insert_str(byte_idx, text);
    text.chars().count()
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
        ConnectionField::Name => {
            let name = state.name.trim();
            if name.is_empty() {
                state
                    .validation_errors
                    .insert(field, "Name is required".to_string());
            } else if name.chars().count() > 50 {
                state
                    .validation_errors
                    .insert(field, "Name must be 50 characters or less".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod insert_str_at_cursor_tests {
        use super::*;

        #[test]
        fn insert_str_at_empty_string() {
            let mut s = String::new();

            let count = insert_str_at_cursor(&mut s, 0, "abc");

            assert_eq!(s, "abc");
            assert_eq!(count, 3);
        }

        #[test]
        fn insert_str_at_beginning() {
            let mut s = "hello".to_string();

            let count = insert_str_at_cursor(&mut s, 0, "xy");

            assert_eq!(s, "xyhello");
            assert_eq!(count, 2);
        }

        #[test]
        fn insert_str_at_middle() {
            let mut s = "abcd".to_string();

            let count = insert_str_at_cursor(&mut s, 2, "XX");

            assert_eq!(s, "abXXcd");
            assert_eq!(count, 2);
        }

        #[test]
        fn insert_str_at_end() {
            let mut s = "abcd".to_string();

            let count = insert_str_at_cursor(&mut s, 4, "!");

            assert_eq!(s, "abcd!");
            assert_eq!(count, 1);
        }

        #[test]
        fn insert_str_with_multibyte() {
            let mut s = "abc".to_string();

            let count = insert_str_at_cursor(&mut s, 1, "日本");

            assert_eq!(s, "a日本bc");
            assert_eq!(count, 2);
        }
    }

    mod validate_field_name {
        use super::*;

        #[test]
        fn empty_name_sets_error() {
            let mut state = ConnectionSetupState {
                name: "".to_string(),
                ..Default::default()
            };

            validate_field(&mut state, ConnectionField::Name);

            assert_eq!(
                state.validation_errors.get(&ConnectionField::Name),
                Some(&"Name is required".to_string())
            );
        }

        #[test]
        fn whitespace_only_name_sets_error() {
            let mut state = ConnectionSetupState {
                name: "   ".to_string(),
                ..Default::default()
            };

            validate_field(&mut state, ConnectionField::Name);

            assert_eq!(
                state.validation_errors.get(&ConnectionField::Name),
                Some(&"Name is required".to_string())
            );
        }

        #[rstest]
        #[case("a".repeat(50), false)]
        #[case("a".repeat(51), true)]
        fn name_length_validation(#[case] name: String, #[case] expect_error: bool) {
            let mut state = ConnectionSetupState {
                name,
                ..Default::default()
            };

            validate_field(&mut state, ConnectionField::Name);

            if expect_error {
                assert_eq!(
                    state.validation_errors.get(&ConnectionField::Name),
                    Some(&"Name must be 50 characters or less".to_string())
                );
            } else {
                assert!(!state.validation_errors.contains_key(&ConnectionField::Name));
            }
        }

        #[test]
        fn valid_name_clears_previous_error() {
            let mut state = ConnectionSetupState {
                name: "".to_string(),
                ..Default::default()
            };
            validate_field(&mut state, ConnectionField::Name);
            assert!(state.validation_errors.contains_key(&ConnectionField::Name));

            state.name = "Valid Name".to_string();
            validate_field(&mut state, ConnectionField::Name);

            assert!(!state.validation_errors.contains_key(&ConnectionField::Name));
        }
    }
}
