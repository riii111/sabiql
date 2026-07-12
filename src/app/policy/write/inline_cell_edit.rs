use std::borrow::Cow;
use std::num::IntErrorKind;

use crate::domain::{DatabaseType, QueryValue};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InlineCellEditError {
    #[error("NULL cells are not editable inline yet")]
    NullUnsupported,
    #[error("BLOB cells are not editable inline")]
    BlobUnsupported,
    #[error("This cell type is not editable inline")]
    UnsupportedCellType,
    #[error("Invalid INTEGER value")]
    InvalidInteger,
    #[error("INTEGER value is out of range")]
    IntegerOverflow,
    #[error("Invalid REAL value")]
    InvalidReal,
    #[error("REAL value must be finite")]
    NonFiniteReal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineCellEditKind {
    Text,
    SqliteInteger,
    SqliteReal,
}

pub fn supports_inline_edit(database_type: DatabaseType, value: &QueryValue) -> bool {
    classify_inline_cell_edit(database_type, value).is_ok()
}

pub fn text_for_inline_edit(
    database_type: DatabaseType,
    value: &QueryValue,
) -> Result<String, InlineCellEditError> {
    match classify_inline_cell_edit(database_type, value)? {
        InlineCellEditKind::Text => match value {
            QueryValue::Text(value) => Ok(value.clone()),
            _ => unreachable!("text edit kind must carry text value"),
        },
        InlineCellEditKind::SqliteInteger | InlineCellEditKind::SqliteReal => {
            Ok(value.display_value())
        }
    }
}

pub fn build_inline_edited_value(
    database_type: DatabaseType,
    original: &QueryValue,
    edited_text: &str,
) -> Result<QueryValue, InlineCellEditError> {
    match classify_inline_cell_edit(database_type, original)? {
        InlineCellEditKind::Text => Ok(QueryValue::text(edited_text)),
        InlineCellEditKind::SqliteInteger => parse_sqlite_integer_text(edited_text),
        InlineCellEditKind::SqliteReal => parse_sqlite_real_text(edited_text),
    }
}

fn classify_inline_cell_edit(
    database_type: DatabaseType,
    value: &QueryValue,
) -> Result<InlineCellEditKind, InlineCellEditError> {
    match database_type {
        DatabaseType::PostgreSQL => classify_postgres_inline_cell_edit(value),
        DatabaseType::SQLite => classify_sqlite_inline_cell_edit(value),
    }
}

fn classify_postgres_inline_cell_edit(
    value: &QueryValue,
) -> Result<InlineCellEditKind, InlineCellEditError> {
    match value {
        QueryValue::Text(_) => Ok(InlineCellEditKind::Text),
        QueryValue::Null => Err(InlineCellEditError::NullUnsupported),
        QueryValue::Blob(_) => Err(InlineCellEditError::BlobUnsupported),
        QueryValue::SqlLiteral(_) => Err(InlineCellEditError::UnsupportedCellType),
    }
}

fn classify_sqlite_inline_cell_edit(
    value: &QueryValue,
) -> Result<InlineCellEditKind, InlineCellEditError> {
    match value {
        QueryValue::Text(_) => Ok(InlineCellEditKind::Text),
        QueryValue::Null => Err(InlineCellEditError::NullUnsupported),
        QueryValue::Blob(_) => Err(InlineCellEditError::BlobUnsupported),
        QueryValue::SqlLiteral(value) => {
            if value.parse::<i64>().is_ok() {
                Ok(InlineCellEditKind::SqliteInteger)
            } else if matches_sqlite_numeric_lexeme(value) {
                Ok(InlineCellEditKind::SqliteReal)
            } else {
                Err(InlineCellEditError::UnsupportedCellType)
            }
        }
    }
}

fn parse_sqlite_integer_text(edited_text: &str) -> Result<QueryValue, InlineCellEditError> {
    edited_text
        .parse::<i64>()
        .map(|value| QueryValue::SqlLiteral(value.to_string()))
        .map_err(|error| {
            if matches!(
                error.kind(),
                IntErrorKind::PosOverflow | IntErrorKind::NegOverflow
            ) {
                InlineCellEditError::IntegerOverflow
            } else {
                InlineCellEditError::InvalidInteger
            }
        })
}

fn parse_sqlite_real_text(edited_text: &str) -> Result<QueryValue, InlineCellEditError> {
    if !matches_sqlite_numeric_lexeme(edited_text) {
        return Err(InlineCellEditError::InvalidReal);
    }

    let parseable = normalize_leading_decimal_zero(edited_text);
    let value = parseable
        .parse::<f64>()
        .map_err(|_| InlineCellEditError::InvalidReal)?;
    if !value.is_finite() {
        return Err(InlineCellEditError::NonFiniteReal);
    }

    Ok(QueryValue::SqlLiteral(normalize_sqlite_real_literal(
        edited_text,
    )))
}

fn normalize_sqlite_real_literal(draft: &str) -> String {
    let normalized = normalize_leading_decimal_zero(draft);
    if normalized.contains('.') || normalized.contains('e') || normalized.contains('E') {
        normalized.into_owned()
    } else {
        format!("{normalized}.0")
    }
}

fn normalize_leading_decimal_zero(value: &str) -> Cow<'_, str> {
    if let Some(rest) = value.strip_prefix("+.") {
        Cow::Owned(format!("+0.{rest}"))
    } else if let Some(rest) = value.strip_prefix("-.") {
        Cow::Owned(format!("-0.{rest}"))
    } else if let Some(rest) = value.strip_prefix('.') {
        Cow::Owned(format!("0.{rest}"))
    } else {
        Cow::Borrowed(value)
    }
}

fn matches_sqlite_numeric_lexeme(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    if matches!(bytes[index], b'+' | b'-') {
        index += 1;
        if index == bytes.len() {
            return false;
        }
    }

    let integer_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    let integer_digits = index - integer_start;

    let fractional_digits = if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let fractional_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        index - fractional_start
    } else {
        0
    };

    if integer_digits == 0 && fractional_digits == 0 {
        return false;
    }

    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        index += 1;
        if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
            index += 1;
        }
        let exponent_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == exponent_start {
            return false;
        }
    }

    index == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_literal_integer_is_inline_editable() {
        assert_eq!(
            text_for_inline_edit(
                DatabaseType::SQLite,
                &QueryValue::SqlLiteral("42".to_string()),
            )
            .unwrap(),
            "42"
        );
    }

    #[test]
    fn text_with_nul_keeps_raw_value_for_editing() {
        assert_eq!(
            text_for_inline_edit(DatabaseType::SQLite, &QueryValue::text("a\0b")).unwrap(),
            "a\0b"
        );
    }

    #[test]
    fn sql_literal_real_accepts_integer_like_draft_and_keeps_real_literal() {
        let value = build_inline_edited_value(
            DatabaseType::SQLite,
            &QueryValue::SqlLiteral("3.14".to_string()),
            "42",
        )
        .unwrap();

        assert_eq!(value, QueryValue::SqlLiteral("42.0".to_string()));
    }

    #[test]
    fn sql_literal_real_accepts_leading_decimal_draft() {
        let value = build_inline_edited_value(
            DatabaseType::SQLite,
            &QueryValue::SqlLiteral("3.14".to_string()),
            ".5",
        )
        .unwrap();

        assert_eq!(value, QueryValue::SqlLiteral("0.5".to_string()));
    }

    #[test]
    fn sql_literal_integer_rejects_overflow() {
        let error = build_inline_edited_value(
            DatabaseType::SQLite,
            &QueryValue::SqlLiteral("7".to_string()),
            "9223372036854775808",
        )
        .unwrap_err();

        assert_eq!(error, InlineCellEditError::IntegerOverflow);
    }

    #[test]
    fn sql_literal_real_rejects_non_finite_input() {
        let error = build_inline_edited_value(
            DatabaseType::SQLite,
            &QueryValue::SqlLiteral("1.0".to_string()),
            "1e999",
        )
        .unwrap_err();

        assert_eq!(error, InlineCellEditError::NonFiniteReal);
    }

    #[test]
    fn sql_literal_real_rejects_non_numeric_input() {
        let error = build_inline_edited_value(
            DatabaseType::SQLite,
            &QueryValue::SqlLiteral("1.0".to_string()),
            "NaN",
        )
        .unwrap_err();

        assert_eq!(error, InlineCellEditError::InvalidReal);
    }

    #[test]
    fn postgres_sql_literal_is_not_treated_as_sqlite_numeric_cell() {
        let error = text_for_inline_edit(
            DatabaseType::PostgreSQL,
            &QueryValue::SqlLiteral("42".to_string()),
        )
        .unwrap_err();

        assert_eq!(error, InlineCellEditError::UnsupportedCellType);
    }
}
