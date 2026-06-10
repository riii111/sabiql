#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        reason = "tests construct fixtures with real clock readings; purity is enforced on production code via the lib target"
    )
)]

pub mod adapters;
pub mod config;
pub mod export;

pub use sabiql_app as app;
pub use sabiql_domain as domain;
