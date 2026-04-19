pub mod adapters;
pub mod event;
pub mod features;
pub mod primitives;
pub mod shell;
pub mod theme;
pub mod tui;

pub use sabiql_app as app;
pub use sabiql_domain as domain;

pub mod ui {
    pub use crate::adapters;
    pub use crate::event;
    pub use crate::features;
    pub use crate::primitives;
    pub use crate::shell;
    pub use crate::theme;
    pub use crate::tui;
}
