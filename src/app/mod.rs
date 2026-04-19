//! Public seams for external layers.
//!
//! The crate root is the composition-root surface for startup/runtime assembly.
//! The `ui` facade is the UI-facing read model and UI-owned outbound port
//! surface. Adapter-facing ports stay under `ports::inbound` and
//! `ports::outbound`.

pub mod catalog;
pub mod cmd;
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
pub use ports::inbound::{InputEvent, InputKeyCombo, Key, Modifiers, handle_input};
pub use runtime::AppRuntime;
pub use services::AppServices;
pub use startup::{StartupLoadError, initialize_connection_state};
pub use update::action::Action;

pub use sabiql_domain as domain;

/// UI crate entrypoint. Keep this limited to the read model and the outbound
/// ports the UI layer actually consumes.
pub mod ui {
    pub use crate::model;
    pub use crate::services::AppServices;
    pub mod ports {
        pub use crate::ports::outbound::{DdlGenerator, RenderOutput, RenderResult, Renderer};
    }
}

#[allow(unused_imports)]
pub(crate) mod app {
    pub(crate) use crate::catalog;
    pub(crate) use crate::cmd;
    pub(crate) use crate::model;
    pub(crate) use crate::palette;
    pub(crate) use crate::policy;
    pub(crate) use crate::ports;
    pub(crate) use crate::runtime;
    pub(crate) use crate::services;
    pub(crate) use crate::startup;
    pub(crate) use crate::update;
}
