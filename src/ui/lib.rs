#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        reason = "tests construct fixtures with real clock readings; purity is enforced on production code via the lib target"
    )
)]
#![cfg_attr(
    test,
    allow(
        unreachable_pub,
        reason = "test support visibility is excluded from production API measurement"
    )
)]

pub mod adapters;
pub mod event;
pub mod features;
pub mod primitives;
pub mod shell;
pub mod theme;
pub mod tui;

pub use sabiql_app as app;
pub use sabiql_domain as domain;
