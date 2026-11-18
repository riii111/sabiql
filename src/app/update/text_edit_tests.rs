use std::sync::Arc;
use std::time::Instant;

use crate::domain::{ConnectionId, DatabaseMetadata, DatabaseType};
use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailState;
use crate::model::connection::setup::ConnectionField;
use crate::model::shared::detail_view::ReadOnlyDetailState;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::text_input::TextInputState;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel};
use crate::services::AppServices;
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind, TextKillDirection};
use crate::update::reducer::reduce;

#[derive(Clone, Copy)]
enum Op {
    Insert(char),
    Backspace,
    Delete,
    Kill(TextKillDirection),
    Yank,
    YankWithEmptyBuffer,
    MoveCursor(CursorMove),
}

impl Op {
    fn label(self) -> String {
        match self {
            Self::Insert(ch) => format!("Insert({ch:?})"),
            Self::Backspace => "Backspace".to_string(),
            Self::Delete => "Delete".to_string(),
            Self::Kill(direction) => format!("Kill({direction:?})"),
            Self::Yank => "Yank".to_string(),
            Self::YankWithEmptyBuffer => "YankWithEmptyBuffer".to_string(),
            Self::MoveCursor(direction) => format!("MoveCursor({direction:?})"),
        }
    }

    fn action(self, target: InputTarget) -> Action {
        match self {
            Self::Insert(ch) => Action::TextInput { target, ch },
            Self::Backspace => Action::TextBackspace { target },
            Self::Delete => Action::TextDelete { target },
            Self::Kill(direction) => Action::TextKill { target, direction },
            Self::Yank | Self::YankWithEmptyBuffer => Action::TextYank { target },
            Self::MoveCursor(direction) => Action::TextMoveCursor { target, direction },
        }
    }
}

const COMMON_OPS: [Op; 9] = [
    Op::Insert('Z'),
    Op::Backspace,
    Op::Delete,
    Op::Kill(TextKillDirection::ToLineStart),
    Op::Kill(TextKillDirection::ToLineEnd),
    Op::Yank,
    Op::YankWithEmptyBuffer,
    Op::MoveCursor(CursorMove::Left),
    Op::MoveCursor(CursorMove::Home),
];

const MULTI_LINE_OPS: [Op; 3] = [
    Op::Insert('\n'),
    Op::Insert('\t'),
    Op::MoveCursor(CursorMove::ViewportBottom),
];

const TARGETS: [InputTarget; 14] = [
    InputTarget::SqlModal,
    InputTarget::SqlModalHighRisk,
    InputTarget::SqlModalAnalyzeHighRisk,
    InputTarget::ResultCellEdit,
    InputTarget::ConnectionSetup,
    InputTarget::CommandLine,
    InputTarget::Filter,
    InputTarget::ErFilter,
    InputTarget::SettingsErBrowser,
    InputTarget::QueryHistoryFilter,
    InputTarget::JsonEdit,
    InputTarget::JsonSearch,
    InputTarget::CellDetailSearch,
    InputTarget::HelpFilter,
];

fn dispatch(state: &mut AppState, action: Action) {
    let _ = reduce(state, action, Instant::now(), &AppServices::stub());
}

fn type_text(state: &mut AppState, target: InputTarget, text: &str) {
    for ch in text.chars() {
        dispatch(state, Action::TextInput { target, ch });
    }
    for _ in 0..2 {
        dispatch(
            state,
            Action::TextMoveCursor {
                target,
                direction: CursorMove::Left,
            },
        );
    }
}

fn connected_state() -> AppState {
    let mut state = AppState::new("test".to_string());
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "postgres",
        DatabaseType::PostgreSQL,
        "postgres://localhost/app",
    );
    state
        .session
        .mark_connected(Arc::new(DatabaseMetadata::new("app".to_string())));
    state
}

fn state_for(target: InputTarget) -> AppState {
    let mut state = connected_state();
    match target {
        InputTarget::SqlModal => {
            state.modal.set_mode(InputMode::SqlModal);
            type_text(&mut state, target, "ab cd");
            dispatch(&mut state, Action::SqlModalEnterNormal);
            let _ = state.sql_modal.consume_completion_debounce();
        }
        InputTarget::SqlModalHighRisk => {
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.begin_confirming_high(
                AdhocRiskDecision {
                    risk_level: RiskLevel::High,
                    label: "DROP",
                },
                "users".to_string(),
            );
            type_text(&mut state, target, "ab cd");
        }
        InputTarget::SqlModalAnalyzeHighRisk => {
            state.modal.set_mode(InputMode::SqlModal);
            state.sql_modal.begin_confirming_analyze_high(
                "EXPLAIN ANALYZE DELETE FROM users".to_string(),
                "users".to_string(),
            );
            type_text(&mut state, target, "ab cd");
        }
        InputTarget::ResultCellEdit => {
            state.modal.set_mode(InputMode::CellEdit);
            state
                .result_interaction
                .begin_cell_edit(0, 0, String::new());
            type_text(&mut state, target, "ab cd");
        }
        InputTarget::ConnectionSetup => {
            state.modal.set_mode(InputMode::ConnectionSetup);
            while state.connection_setup.focused_field() != ConnectionField::Host {
                state.connection_setup.focus_next_field();
            }
            state.connection_setup.host.set_content(String::new());
            type_text(&mut state, target, "ab cd");
        }
        InputTarget::CommandLine => {
            dispatch(&mut state, Action::EnterCommandLine);
            type_text(&mut state, target, "ab cd");
        }
        InputTarget::Filter => {
            state.modal.set_mode(InputMode::TablePicker);
            type_text(&mut state, target, "ab cd");
            state.ui.table_picker_mut().set_selection(1);
        }
        InputTarget::ErFilter => {
            state.modal.set_mode(InputMode::ErTablePicker);
            type_text(&mut state, target, "ab cd");
            state.ui.er_picker_mut().set_selection(1);
        }
        InputTarget::SettingsErBrowser => {
            dispatch(&mut state, Action::OpenModal(ModalKind::Settings));
            while !state.settings.is_editing_custom_er_browser() {
                dispatch(&mut state, Action::SettingsNextSection);
                dispatch(&mut state, Action::SettingsStartCustomBrowserEdit);
            }
            type_text(&mut state, target, "ab cd");
        }
        InputTarget::QueryHistoryFilter => {
            state.modal.set_mode(InputMode::QueryHistoryPicker);
            type_text(&mut state, target, "ab cd");
            state.query_history_picker.set_selection_for_test(1);
        }
        InputTarget::JsonEdit => {
            state.json_detail =
                JsonDetailState::open(0, 0, "settings".to_string(), r#"{"a":1}"#.to_string());
            state.json_detail.enter_edit();
            state.modal.set_mode(InputMode::JsonEdit);
            for _ in 0..3 {
                dispatch(
                    &mut state,
                    Action::TextMoveCursor {
                        target,
                        direction: CursorMove::Right,
                    },
                );
            }
        }
        InputTarget::JsonSearch => {
            state.json_detail = JsonDetailState::open(
                0,
                0,
                "settings".to_string(),
                r#"{"ab":"ab cd"}"#.to_string(),
            );
            state.json_detail.enter_search();
            state.modal.set_mode(InputMode::JsonDetail);
            type_text(&mut state, target, "ab cd");
        }
        InputTarget::CellDetailSearch => {
            state.cell_detail = ReadOnlyDetailState::open(
                0,
                0,
                "note".to_string(),
                "ab cd ab".to_string(),
                "ab cd ab".to_string(),
            );
            state.cell_detail.enter_search();
            state.modal.set_mode(InputMode::CellDetail);
            type_text(&mut state, target, "ab cd");
        }
        InputTarget::HelpFilter => {
            dispatch(&mut state, Action::ToggleModal(ModalKind::Help));
            dispatch(&mut state, Action::EnterHelpFilter);
            type_text(&mut state, target, "ab cd");
        }
    }
    state
}

fn single_line(input: &TextInputState) -> String {
    format!("{:?}@{}", input.content(), input.cursor())
}

fn observe(state: &AppState, target: InputTarget) -> String {
    let observed = match target {
        InputTarget::SqlModal => {
            let editor = state.sql_modal.editor();
            format!(
                "{:?}@{} status={:?} debounce={}",
                editor.content(),
                editor.cursor(),
                std::mem::discriminant(state.sql_modal.status()),
                state.sql_modal.completion_debounce().is_some()
            )
        }
        InputTarget::SqlModalHighRisk | InputTarget::SqlModalAnalyzeHighRisk => {
            match state.sql_modal.status() {
                SqlModalStatus::ConfirmingHigh { input, .. }
                | SqlModalStatus::ConfirmingAnalyzeHigh { input, .. } => single_line(input),
                other => format!("status={other:?}"),
            }
        }
        InputTarget::ResultCellEdit => single_line(state.result_interaction.cell_edit().input()),
        InputTarget::ConnectionSetup => state
            .connection_setup
            .focused_input()
            .map_or_else(|| "no focused input".to_string(), single_line),
        InputTarget::CommandLine => single_line(&state.command_line_input),
        InputTarget::Filter => format!(
            "{} selected={}",
            single_line(state.ui.table_picker().filter_input()),
            state.ui.table_picker().selected()
        ),
        InputTarget::ErFilter => format!(
            "{} selected={}",
            single_line(state.ui.er_picker().filter_input()),
            state.ui.er_picker().selected()
        ),
        InputTarget::SettingsErBrowser => single_line(state.settings.custom_er_browser()),
        InputTarget::QueryHistoryFilter => format!(
            "{} selected={}",
            single_line(state.query_history_picker.filter_input()),
            state.query_history_picker.selected()
        ),
        InputTarget::JsonEdit => {
            let editor = state.json_detail.editor();
            format!(
                "{:?}@{} invalid={}",
                editor.content(),
                editor.cursor(),
                state.json_detail.validation_error().is_some()
            )
        }
        InputTarget::JsonSearch => format!(
            "{} matches={:?}",
            single_line(state.json_detail.search().input()),
            state.json_detail.search().matches()
        ),
        InputTarget::CellDetailSearch => format!(
            "{} matches={:?}",
            single_line(state.cell_detail.search().input()),
            state.cell_detail.search().matches()
        ),
        InputTarget::HelpFilter => single_line(state.ui.help().filter()),
    };
    format!("{observed} kill={:?}", state.kill_buffer())
}

fn ops_for(target: InputTarget) -> Vec<Op> {
    let mut ops = COMMON_OPS.to_vec();
    if matches!(target, InputTarget::SqlModal | InputTarget::JsonEdit) {
        ops.extend(MULTI_LINE_OPS);
    }
    if target == InputTarget::CellDetailSearch {
        ops.retain(|op| !matches!(op, Op::Kill(_) | Op::Yank | Op::YankWithEmptyBuffer));
    }
    ops
}

fn text_edit_report() -> String {
    let mut lines = Vec::new();
    for target in TARGETS {
        lines.push(format!(
            "{target:?} initial: {}",
            observe(&state_for(target), target)
        ));
        for op in ops_for(target) {
            let mut state = state_for(target);
            if !matches!(op, Op::YankWithEmptyBuffer) {
                state.record_kill("XY".to_string());
            }
            dispatch(&mut state, op.action(target));
            lines.push(format!(
                "{target:?} {}: {}",
                op.label(),
                observe(&state, target)
            ));
        }
    }
    lines.join("\n")
}

mod paste_outside_search {
    use super::*;

    #[test]
    fn json_detail_viewing_ignores_paste() {
        let mut state = state_for(InputTarget::JsonSearch);
        dispatch(&mut state, Action::JsonExitSearch);
        let before = observe(&state, InputTarget::JsonSearch);

        dispatch(&mut state, Action::Paste("pasted".to_string()));

        assert_eq!(state.input_mode(), InputMode::JsonDetail);
        assert_eq!(observe(&state, InputTarget::JsonSearch), before);
    }

    #[test]
    fn cell_detail_viewing_ignores_paste() {
        let mut state = state_for(InputTarget::CellDetailSearch);
        dispatch(&mut state, Action::CellDetailExitSearch);
        let before = observe(&state, InputTarget::CellDetailSearch);

        dispatch(&mut state, Action::Paste("pasted".to_string()));

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(observe(&state, InputTarget::CellDetailSearch), before);
    }
}

#[test]
fn text_edits_keep_per_target_behavior() {
    let report = text_edit_report();

    assert_eq!(report, EXPECTED_REPORT, "\n{report}\n");
}

const EXPECTED_REPORT: &str = r#"SqlModal initial: "ab cd"@3 status=Discriminant(0) debounce=false kill=None
SqlModal Insert('Z'): "ab Zcd"@4 status=Discriminant(1) debounce=true kill=Some("XY")
SqlModal Backspace: "abcd"@2 status=Discriminant(1) debounce=true kill=Some("XY")
SqlModal Delete: "ab d"@3 status=Discriminant(1) debounce=true kill=Some("XY")
SqlModal Kill(ToLineStart): "cd"@0 status=Discriminant(1) debounce=true kill=Some("ab ")
SqlModal Kill(ToLineEnd): "ab "@3 status=Discriminant(1) debounce=true kill=Some("cd")
SqlModal Yank: "ab XYcd"@5 status=Discriminant(1) debounce=true kill=Some("XY")
SqlModal YankWithEmptyBuffer: "ab cd"@3 status=Discriminant(0) debounce=false kill=None
SqlModal MoveCursor(Left): "ab cd"@2 status=Discriminant(0) debounce=false kill=Some("XY")
SqlModal MoveCursor(Home): "ab cd"@0 status=Discriminant(0) debounce=false kill=Some("XY")
SqlModal Insert('\n'): "ab \ncd"@4 status=Discriminant(1) debounce=true kill=Some("XY")
SqlModal Insert('\t'): "ab \tcd"@4 status=Discriminant(1) debounce=true kill=Some("XY")
SqlModal MoveCursor(ViewportBottom): "ab cd"@3 status=Discriminant(0) debounce=false kill=Some("XY")
SqlModalHighRisk initial: "ab cd"@3 kill=None
SqlModalHighRisk Insert('Z'): "ab Zcd"@4 kill=Some("XY")
SqlModalHighRisk Backspace: "abcd"@2 kill=Some("XY")
SqlModalHighRisk Delete: "ab d"@3 kill=Some("XY")
SqlModalHighRisk Kill(ToLineStart): "cd"@0 kill=Some("ab ")
SqlModalHighRisk Kill(ToLineEnd): "ab "@3 kill=Some("cd")
SqlModalHighRisk Yank: "ab XYcd"@5 kill=Some("XY")
SqlModalHighRisk YankWithEmptyBuffer: "ab cd"@3 kill=None
SqlModalHighRisk MoveCursor(Left): "ab cd"@2 kill=Some("XY")
SqlModalHighRisk MoveCursor(Home): "ab cd"@0 kill=Some("XY")
SqlModalAnalyzeHighRisk initial: "ab cd"@3 kill=None
SqlModalAnalyzeHighRisk Insert('Z'): "ab Zcd"@4 kill=Some("XY")
SqlModalAnalyzeHighRisk Backspace: "abcd"@2 kill=Some("XY")
SqlModalAnalyzeHighRisk Delete: "ab d"@3 kill=Some("XY")
SqlModalAnalyzeHighRisk Kill(ToLineStart): "cd"@0 kill=Some("ab ")
SqlModalAnalyzeHighRisk Kill(ToLineEnd): "ab "@3 kill=Some("cd")
SqlModalAnalyzeHighRisk Yank: "ab XYcd"@5 kill=Some("XY")
SqlModalAnalyzeHighRisk YankWithEmptyBuffer: "ab cd"@3 kill=None
SqlModalAnalyzeHighRisk MoveCursor(Left): "ab cd"@2 kill=Some("XY")
SqlModalAnalyzeHighRisk MoveCursor(Home): "ab cd"@0 kill=Some("XY")
ResultCellEdit initial: "ab cd"@3 kill=None
ResultCellEdit Insert('Z'): "ab Zcd"@4 kill=Some("XY")
ResultCellEdit Backspace: "abcd"@2 kill=Some("XY")
ResultCellEdit Delete: "ab d"@3 kill=Some("XY")
ResultCellEdit Kill(ToLineStart): "cd"@0 kill=Some("ab ")
ResultCellEdit Kill(ToLineEnd): "ab "@3 kill=Some("cd")
ResultCellEdit Yank: "ab XYcd"@5 kill=Some("XY")
ResultCellEdit YankWithEmptyBuffer: "ab cd"@3 kill=None
ResultCellEdit MoveCursor(Left): "ab cd"@2 kill=Some("XY")
ResultCellEdit MoveCursor(Home): "ab cd"@0 kill=Some("XY")
ConnectionSetup initial: "ab cd"@3 kill=None
ConnectionSetup Insert('Z'): "ab Zcd"@4 kill=Some("XY")
ConnectionSetup Backspace: "abcd"@2 kill=Some("XY")
ConnectionSetup Delete: "ab d"@3 kill=Some("XY")
ConnectionSetup Kill(ToLineStart): "cd"@0 kill=Some("ab ")
ConnectionSetup Kill(ToLineEnd): "ab "@3 kill=Some("cd")
ConnectionSetup Yank: "ab XYcd"@5 kill=Some("XY")
ConnectionSetup YankWithEmptyBuffer: "ab cd"@3 kill=None
ConnectionSetup MoveCursor(Left): "ab cd"@2 kill=Some("XY")
ConnectionSetup MoveCursor(Home): "ab cd"@0 kill=Some("XY")
CommandLine initial: "ab cd"@3 kill=None
CommandLine Insert('Z'): "ab Zcd"@4 kill=Some("XY")
CommandLine Backspace: "abcd"@2 kill=Some("XY")
CommandLine Delete: "ab d"@3 kill=Some("XY")
CommandLine Kill(ToLineStart): "cd"@0 kill=Some("ab ")
CommandLine Kill(ToLineEnd): "ab "@3 kill=Some("cd")
CommandLine Yank: "ab XYcd"@5 kill=Some("XY")
CommandLine YankWithEmptyBuffer: "ab cd"@3 kill=None
CommandLine MoveCursor(Left): "ab cd"@2 kill=Some("XY")
CommandLine MoveCursor(Home): "ab cd"@0 kill=Some("XY")
Filter initial: "ab cd"@3 selected=1 kill=None
Filter Insert('Z'): "ab Zcd"@4 selected=0 kill=Some("XY")
Filter Backspace: "abcd"@2 selected=0 kill=Some("XY")
Filter Delete: "ab d"@3 selected=0 kill=Some("XY")
Filter Kill(ToLineStart): "cd"@0 selected=0 kill=Some("ab ")
Filter Kill(ToLineEnd): "ab "@3 selected=0 kill=Some("cd")
Filter Yank: "ab XYcd"@5 selected=0 kill=Some("XY")
Filter YankWithEmptyBuffer: "ab cd"@3 selected=1 kill=None
Filter MoveCursor(Left): "ab cd"@2 selected=1 kill=Some("XY")
Filter MoveCursor(Home): "ab cd"@0 selected=1 kill=Some("XY")
ErFilter initial: "ab cd"@3 selected=1 kill=None
ErFilter Insert('Z'): "ab Zcd"@4 selected=0 kill=Some("XY")
ErFilter Backspace: "abcd"@2 selected=0 kill=Some("XY")
ErFilter Delete: "ab d"@3 selected=0 kill=Some("XY")
ErFilter Kill(ToLineStart): "cd"@0 selected=0 kill=Some("ab ")
ErFilter Kill(ToLineEnd): "ab "@3 selected=0 kill=Some("cd")
ErFilter Yank: "ab XYcd"@5 selected=0 kill=Some("XY")
ErFilter YankWithEmptyBuffer: "ab cd"@3 selected=1 kill=None
ErFilter MoveCursor(Left): "ab cd"@2 selected=1 kill=Some("XY")
ErFilter MoveCursor(Home): "ab cd"@0 selected=1 kill=Some("XY")
SettingsErBrowser initial: "ab cd"@3 kill=None
SettingsErBrowser Insert('Z'): "ab Zcd"@4 kill=Some("XY")
SettingsErBrowser Backspace: "abcd"@2 kill=Some("XY")
SettingsErBrowser Delete: "ab d"@3 kill=Some("XY")
SettingsErBrowser Kill(ToLineStart): "cd"@0 kill=Some("ab ")
SettingsErBrowser Kill(ToLineEnd): "ab "@3 kill=Some("cd")
SettingsErBrowser Yank: "ab XYcd"@5 kill=Some("XY")
SettingsErBrowser YankWithEmptyBuffer: "ab cd"@3 kill=None
SettingsErBrowser MoveCursor(Left): "ab cd"@2 kill=Some("XY")
SettingsErBrowser MoveCursor(Home): "ab cd"@0 kill=Some("XY")
QueryHistoryFilter initial: "ab cd"@3 selected=1 kill=None
QueryHistoryFilter Insert('Z'): "ab Zcd"@4 selected=0 kill=Some("XY")
QueryHistoryFilter Backspace: "abcd"@2 selected=0 kill=Some("XY")
QueryHistoryFilter Delete: "ab d"@3 selected=0 kill=Some("XY")
QueryHistoryFilter Kill(ToLineStart): "cd"@0 selected=0 kill=Some("ab ")
QueryHistoryFilter Kill(ToLineEnd): "ab "@3 selected=0 kill=Some("cd")
QueryHistoryFilter Yank: "ab XYcd"@5 selected=0 kill=Some("XY")
QueryHistoryFilter YankWithEmptyBuffer: "ab cd"@3 selected=1 kill=None
QueryHistoryFilter MoveCursor(Left): "ab cd"@2 selected=1 kill=Some("XY")
QueryHistoryFilter MoveCursor(Home): "ab cd"@0 selected=1 kill=Some("XY")
JsonEdit initial: "{\n  \"a\": 1\n}"@3 invalid=false kill=None
JsonEdit Insert('Z'): "{\n Z \"a\": 1\n}"@4 invalid=true kill=Some("XY")
JsonEdit Backspace: "{\n \"a\": 1\n}"@2 invalid=false kill=Some("XY")
JsonEdit Delete: "{\n \"a\": 1\n}"@3 invalid=false kill=Some("XY")
JsonEdit Kill(ToLineStart): "{\n \"a\": 1\n}"@2 invalid=false kill=Some(" ")
JsonEdit Kill(ToLineEnd): "{\n \n}"@3 invalid=false kill=Some(" \"a\": 1")
JsonEdit Yank: "{\n XY \"a\": 1\n}"@5 invalid=true kill=Some("XY")
JsonEdit YankWithEmptyBuffer: "{\n  \"a\": 1\n}"@3 invalid=false kill=None
JsonEdit MoveCursor(Left): "{\n  \"a\": 1\n}"@2 invalid=false kill=Some("XY")
JsonEdit MoveCursor(Home): "{\n  \"a\": 1\n}"@2 invalid=false kill=Some("XY")
JsonEdit Insert('\n'): "{\n \n \"a\": 1\n}"@4 invalid=false kill=Some("XY")
JsonEdit Insert('\t'): "{\n      \"a\": 1\n}"@7 invalid=false kill=Some("XY")
JsonEdit MoveCursor(ViewportBottom): "{\n  \"a\": 1\n}"@12 invalid=false kill=Some("XY")
JsonSearch initial: "ab cd"@3 matches=[11] kill=None
JsonSearch Insert('Z'): "ab Zcd"@4 matches=[] kill=Some("XY")
JsonSearch Backspace: "abcd"@2 matches=[] kill=Some("XY")
JsonSearch Delete: "ab d"@3 matches=[] kill=Some("XY")
JsonSearch Kill(ToLineStart): "cd"@0 matches=[14] kill=Some("ab ")
JsonSearch Kill(ToLineEnd): "ab "@3 matches=[11] kill=Some("cd")
JsonSearch Yank: "ab XYcd"@5 matches=[] kill=Some("XY")
JsonSearch YankWithEmptyBuffer: "ab cd"@3 matches=[11] kill=None
JsonSearch MoveCursor(Left): "ab cd"@2 matches=[11] kill=Some("XY")
JsonSearch MoveCursor(Home): "ab cd"@0 matches=[11] kill=Some("XY")
CellDetailSearch initial: "ab cd"@3 matches=[0] kill=None
CellDetailSearch Insert('Z'): "ab Zcd"@4 matches=[] kill=Some("XY")
CellDetailSearch Backspace: "abcd"@2 matches=[] kill=Some("XY")
CellDetailSearch Delete: "ab d"@3 matches=[] kill=Some("XY")
CellDetailSearch MoveCursor(Left): "ab cd"@2 matches=[0] kill=Some("XY")
CellDetailSearch MoveCursor(Home): "ab cd"@0 matches=[0] kill=Some("XY")
HelpFilter initial: "ab cd"@3 kill=None
HelpFilter Insert('Z'): "ab Zcd"@4 kill=Some("XY")
HelpFilter Backspace: "abcd"@2 kill=Some("XY")
HelpFilter Delete: "ab d"@3 kill=Some("XY")
HelpFilter Kill(ToLineStart): "cd"@0 kill=Some("ab ")
HelpFilter Kill(ToLineEnd): "ab "@3 kill=Some("cd")
HelpFilter Yank: "ab XYcd"@5 kill=Some("XY")
HelpFilter YankWithEmptyBuffer: "ab cd"@3 kill=None
HelpFilter MoveCursor(Left): "ab cd"@2 kill=Some("XY")
HelpFilter MoveCursor(Home): "ab cd"@0 kill=Some("XY")"#;
