//! Test-only fixture builders for app unit tests.
//!
//! # Closure fixture builders
//!
//! Prefer [`column::column_fixture`] and [`table::table_fixture`] when a fixture is
//! complex, reused across tests, or has many field override patterns. The closure
//! makes the overridden fields visible at the call site:
//!
//! ```ignore
//! column_fixture(|c| {
//!     c.name = "id".into();
//!     c.data_type = "integer".into();
//!     c.ordinal_position = 1;
//!     c.attributes = ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE;
//! });
//! ```
//!
//! Keep small one-off literals or struct updates when every field is part of the
//! test subject. Avoid positional helpers such as `helper(name, type, ordinal)` —
//! test defaults belong here, not on production types.

pub mod column;
pub mod table;
