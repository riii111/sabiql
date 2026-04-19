pub mod cmd;
pub mod input;
pub mod model;
pub mod palette;
pub mod policy;
pub mod runtime;
pub mod startup;
pub mod update;

pub mod ports;
pub mod services;

pub use sabiql_domain as domain;

pub mod app {
    pub use crate::cmd;
    pub use crate::input;
    pub use crate::model;
    pub use crate::palette;
    pub use crate::policy;
    pub use crate::ports;
    pub use crate::runtime;
    pub use crate::services;
    pub use crate::startup;
    pub use crate::update;
}
