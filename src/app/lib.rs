#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        reason = "tests construct fixtures with real clock readings; purity is enforced on production code via the lib target"
    )
)]

pub mod catalog;
pub mod cmd;
pub mod model;
pub mod policy;
pub mod update;

pub mod ports;
pub mod services;

#[cfg(test)]
pub(crate) mod test_support;

pub use sabiql_domain as domain;
