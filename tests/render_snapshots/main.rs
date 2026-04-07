#[path = "../harness/mod.rs"]
mod harness;

use harness::fixtures;
use harness::{
    create_test_state, create_test_terminal, create_test_terminal_sized, render_and_get_buffer_at,
    render_to_string, test_instant,
};

use std::sync::Arc;
use std::time::Duration;

use sabiql::app::model::connection::error::{ConnectionErrorInfo, ConnectionErrorKind};
use sabiql::app::model::connection::setup::ConnectionField;
use sabiql::app::model::er_state::ErStatus;
use sabiql::app::model::shared::focused_pane::FocusedPane;
use sabiql::app::model::shared::input_mode::InputMode;
use sabiql::app::model::shared::text_input::TextInputState;
use sabiql::app::model::sql_editor::completion::{CompletionCandidate, CompletionKind};
use sabiql::app::model::sql_editor::modal::{AdhocSuccessSnapshot, SqlModalStatus, SqlModalTab};
use sabiql::app::policy::json::json_diff::compute_json_diff;
use sabiql::app::policy::write::write_guardrails::{
    AdhocRiskDecision, ColumnDiff, GuardrailDecision, RiskLevel, TargetSummary, WriteOperation,
    WritePreview,
};
use sabiql::app::policy::write::write_update::normalize_for_diff;
use sabiql::domain::{CommandTag, QuerySource};

mod confirm_dialogs;
mod connection_flow;
mod connection_management;
mod er_diagram;
mod initial_state;
mod inspector;
mod overlays;
mod result_history;
mod result_pane;
mod style_assertions;
mod table_explorer;
