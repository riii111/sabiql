use crate::tests::harness;

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

#[path = "../../../tests/render_snapshots/confirm_dialogs.rs"]
mod confirm_dialogs;
#[path = "../../../tests/render_snapshots/connection_flow.rs"]
mod connection_flow;
#[path = "../../../tests/render_snapshots/connection_management.rs"]
mod connection_management;
#[path = "../../../tests/render_snapshots/er_diagram.rs"]
mod er_diagram;
#[path = "../../../tests/render_snapshots/initial_state.rs"]
mod initial_state;
#[path = "../../../tests/render_snapshots/inspector.rs"]
mod inspector;
#[path = "../../../tests/render_snapshots/overlays.rs"]
mod overlays;
#[path = "../../../tests/render_snapshots/result_history.rs"]
mod result_history;
#[path = "../../../tests/render_snapshots/result_pane.rs"]
mod result_pane;
#[path = "../../../tests/render_snapshots/style_assertions.rs"]
mod style_assertions;
#[path = "../../../tests/render_snapshots/table_explorer.rs"]
mod table_explorer;
