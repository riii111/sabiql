pub mod cmd;
pub mod catalog;
pub mod model;
pub mod palette;
pub mod policy;
pub mod runtime;
pub mod startup;
pub mod update;

pub mod ports;
pub mod services;

pub use cmd::cache::TtlCache;
pub use cmd::completion_engine::CompletionEngine;
pub use cmd::render_schedule::next_animation_deadline;
pub use cmd::runner::EffectRunner;
pub use model::app_state::AppState;
pub use model::shared::db_capabilities::DbCapabilities;
pub use model::shared::input_mode::InputMode;
pub use ports::{InputEvent, InputKeyCombo, Key, Modifiers, handle_input};
pub use runtime::AppRuntime;
pub use services::AppServices;
pub use startup::{StartupLoadError, initialize_connection_state};
pub use update::action::Action;

pub use sabiql_domain as domain;

pub mod ui {
    pub use crate::model;
    pub use crate::ports;
}

pub mod app {
    pub use crate::cmd;
    pub use crate::catalog;
    pub use crate::model;
    pub use crate::palette;
    pub use crate::policy;
    pub use crate::ports;
    pub use crate::runtime;
    pub use crate::services;
    pub use crate::startup;
    pub use crate::update;
}
