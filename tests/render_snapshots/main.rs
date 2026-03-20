#[path = "../harness/mod.rs"]
mod harness;

use harness::fixtures;
use harness::{
    create_test_state, create_test_terminal, render_and_get_buffer, render_to_string, test_instant,
};

use std::sync::Arc;
use std::time::Duration;

use sabiql::app::connection_error::{ConnectionErrorInfo, ConnectionErrorKind};
use sabiql::app::connection_setup_state::ConnectionField;
use sabiql::app::er_state::ErStatus;
use sabiql::app::focused_pane::FocusedPane;
use sabiql::app::input_mode::InputMode;
use sabiql::app::sql_modal_context::{
    AdhocSuccessSnapshot, CompletionCandidate, CompletionKind, SqlModalStatus, SqlModalTab,
};
use sabiql::app::text_input::TextInputState;
use sabiql::app::write_guardrails::{
    AdhocRiskDecision, ColumnDiff, GuardrailDecision, RiskLevel, TargetSummary, WriteOperation,
    WritePreview,
};
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
