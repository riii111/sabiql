//! Test-only fixture helpers for app unit tests.
//!
//! # Closure fixture builders
//!
//! Prefer [`column::column_fixture`] when a column fixture is complex, reused across
//! tests, or has field override patterns. The closure makes the overridden fields
//! visible at the call site:
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
//! For tables, use a struct literal with [`table::minimal`] for unrelated fields.
//! For small one-off columns where every field is part of the test subject, keep a
//! struct literal or a small helper such as [`column::test_nullable_column`]. Test
//! defaults belong here, not on production types.

pub mod column;
pub mod table;
