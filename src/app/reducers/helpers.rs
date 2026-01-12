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

    mod validate_field_name {
        use super::*;

        #[test]
        fn empty_name_sets_error() {
            let mut state = ConnectionSetupState::default();
            state.name = "".to_string();

            validate_field(&mut state, ConnectionField::Name);

            assert_eq!(
                state.validation_errors.get(&ConnectionField::Name),
                Some(&"Name is required".to_string())
            );
        }

        #[test]
        fn whitespace_only_name_sets_error() {
            let mut state = ConnectionSetupState::default();
            state.name = "   ".to_string();

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
            let mut state = ConnectionSetupState::default();
            state.name = name;

            validate_field(&mut state, ConnectionField::Name);

            if expect_error {
                assert_eq!(
                    state.validation_errors.get(&ConnectionField::Name),
                    Some(&"Name must be 50 characters or less".to_string())
                );
            } else {
                assert!(
                    state
                        .validation_errors
                        .get(&ConnectionField::Name)
                        .is_none()
                );
            }
        }

        #[test]
        fn valid_name_clears_previous_error() {
            let mut state = ConnectionSetupState::default();
            state.name = "".to_string();
            validate_field(&mut state, ConnectionField::Name);
            assert!(state.validation_errors.contains_key(&ConnectionField::Name));

            state.name = "Valid Name".to_string();
            validate_field(&mut state, ConnectionField::Name);

            assert!(!state.validation_errors.contains_key(&ConnectionField::Name));
        }
    }
}
