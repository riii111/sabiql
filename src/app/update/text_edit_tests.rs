use crate::test_support::endpoint;
use std::sync::Arc;
use std::time::Instant;

use crate::domain::{ConnectionId, DatabaseMetadata};
use crate::model::app_state::AppState;
use crate::model::browse::json_detail::JsonDetailState;
use crate::model::connection::setup::ConnectionField;
use crate::model::shared::detail_view::ReadOnlyDetailState;
use crate::model::shared::input_mode::InputMode;
use crate::model::sql_editor::modal::SqlModalStatus;
use crate::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel};
use crate::services::AppServices;
use crate::update::action::DetailTarget;
use crate::update::action::SearchStep;
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind, TextKillDirection};
use crate::update::reducer::reduce;

#[derive(Clone, Copy, Debug)]
enum Step {
    Initial,
    Insert(char),
    Backspace,
    Delete,
    KillToLineStart,
    KillToLineEnd,
    Yank,
    YankWithEmptyBuffer,
    Left,
    Home,
    ViewportBottom,
}

impl Step {
    fn action(self, target: InputTarget) -> Option<Action> {
        let action = match self {
            Self::Initial => return None,
            Self::Insert(ch) => Action::TextInput { target, ch },
            Self::Backspace => Action::TextBackspace { target },
            Self::Delete => Action::TextDelete { target },
            Self::KillToLineStart => Action::TextKill {
                target,
                direction: TextKillDirection::ToLineStart,
            },
            Self::KillToLineEnd => Action::TextKill {
                target,
                direction: TextKillDirection::ToLineEnd,
            },
            Self::Yank | Self::YankWithEmptyBuffer => Action::TextYank { target },
            Self::Left => Action::TextMoveCursor {
                target,
                direction: CursorMove::Left,
            },
            Self::Home => Action::TextMoveCursor {
                target,
                direction: CursorMove::Home,
            },
            Self::ViewportBottom => Action::TextMoveCursor {
                target,
                direction: CursorMove::ViewportBottom,
            },
        };
        Some(action)
    }

    fn edits_text(self) -> bool {
        matches!(
            self,
            Self::Insert(_)
                | Self::Backspace
                | Self::Delete
                | Self::KillToLineStart
                | Self::KillToLineEnd
                | Self::Yank
        )
    }
}

#[derive(Clone, Copy)]
struct Case {
    step: Step,
    content: &'static str,
    cursor: usize,
    kill: Option<&'static str>,
}

const fn case(
    step: Step,
    content: &'static str,
    cursor: usize,
    kill: Option<&'static str>,
) -> Case {
    Case {
        step,
        content,
        cursor,
        kill,
    }
}

const SINGLE_LINE_CASES: [Case; 10] = [
    case(Step::Initial, "ab cd", 3, None),
    case(Step::Insert('Z'), "ab Zcd", 4, Some("XY")),
    case(Step::Backspace, "abcd", 2, Some("XY")),
    case(Step::Delete, "ab d", 3, Some("XY")),
    case(Step::KillToLineStart, "cd", 0, Some("ab ")),
    case(Step::KillToLineEnd, "ab ", 3, Some("cd")),
    case(Step::Yank, "ab XYcd", 5, Some("XY")),
    case(Step::YankWithEmptyBuffer, "ab cd", 3, None),
    case(Step::Left, "ab cd", 2, Some("XY")),
    case(Step::Home, "ab cd", 0, Some("XY")),
];

const SQL_MODAL_MULTI_LINE_CASES: [Case; 3] = [
    case(Step::Insert('\n'), "ab \ncd", 4, Some("XY")),
    case(Step::Insert('\t'), "ab \tcd", 4, Some("XY")),
    case(Step::ViewportBottom, "ab cd", 3, Some("XY")),
];

const JSON_EDIT_CASES: [Case; 13] = [
    case(Step::Initial, "{\n  \"a\": 1\n}", 3, None),
    case(Step::Insert('Z'), "{\n Z \"a\": 1\n}", 4, Some("XY")),
    case(Step::Backspace, "{\n \"a\": 1\n}", 2, Some("XY")),
    case(Step::Delete, "{\n \"a\": 1\n}", 3, Some("XY")),
    case(Step::KillToLineStart, "{\n \"a\": 1\n}", 2, Some(" ")),
    case(Step::KillToLineEnd, "{\n \n}", 3, Some(" \"a\": 1")),
    case(Step::Yank, "{\n XY \"a\": 1\n}", 5, Some("XY")),
    case(Step::YankWithEmptyBuffer, "{\n  \"a\": 1\n}", 3, None),
    case(Step::Left, "{\n  \"a\": 1\n}", 2, Some("XY")),
    case(Step::Home, "{\n  \"a\": 1\n}", 2, Some("XY")),
    case(Step::Insert('\n'), "{\n \n \"a\": 1\n}", 4, Some("XY")),
    case(Step::Insert('\t'), "{\n      \"a\": 1\n}", 7, Some("XY")),
    case(Step::ViewportBottom, "{\n  \"a\": 1\n}", 12, Some("XY")),
];

const SELECTION_BEFORE_EDIT: usize = 1;

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
    state.session.activate_connection(
        &ConnectionId::new(),
        "postgres",
        &endpoint::postgres("postgres://localhost/app"),
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
            state
                .ui
                .table_picker_mut()
                .set_selection(SELECTION_BEFORE_EDIT);
        }
        InputTarget::ErFilter => {
            state.modal.set_mode(InputMode::ErTablePicker);
            type_text(&mut state, target, "ab cd");
            state
                .ui
                .er_picker_mut()
                .set_selection(SELECTION_BEFORE_EDIT);
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
            state
                .query_history_picker
                .set_selection_for_test(SELECTION_BEFORE_EDIT);
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
            dispatch(&mut state, Action::ToggleHelp);
            dispatch(&mut state, Action::EnterHelpFilter);
            type_text(&mut state, target, "ab cd");
        }
    }
    state
}

fn state_after(target: InputTarget, step: Step) -> AppState {
    let mut state = state_for(target);
    if let Some(action) = step.action(target) {
        if !matches!(step, Step::YankWithEmptyBuffer) {
            state.record_kill("XY".to_string());
        }
        dispatch(&mut state, action);
    }
    state
}

fn cases_for(target: InputTarget) -> Vec<Case> {
    match target {
        InputTarget::SqlModal => {
            [SINGLE_LINE_CASES.as_slice(), &SQL_MODAL_MULTI_LINE_CASES].concat()
        }
        InputTarget::JsonEdit => JSON_EDIT_CASES.to_vec(),
        InputTarget::CellDetailSearch => SINGLE_LINE_CASES
            .into_iter()
            .filter(|case| {
                !matches!(
                    case.step,
                    Step::KillToLineStart
                        | Step::KillToLineEnd
                        | Step::Yank
                        | Step::YankWithEmptyBuffer
                )
            })
            .collect(),
        InputTarget::SqlModalHighRisk
        | InputTarget::SqlModalAnalyzeHighRisk
        | InputTarget::ResultCellEdit
        | InputTarget::ConnectionSetup
        | InputTarget::CommandLine
        | InputTarget::Filter
        | InputTarget::ErFilter
        | InputTarget::SettingsErBrowser
        | InputTarget::QueryHistoryFilter
        | InputTarget::JsonSearch
        | InputTarget::HelpFilter => SINGLE_LINE_CASES.to_vec(),
    }
}

fn edited_text(state: &AppState, target: InputTarget) -> Option<(&str, usize)> {
    let input = match target {
        InputTarget::SqlModal => {
            let editor = state.sql_modal.editor();
            return Some((editor.content(), editor.cursor()));
        }
        InputTarget::JsonEdit => {
            let editor = state.json_detail.editor();
            return Some((editor.content(), editor.cursor()));
        }
        InputTarget::SqlModalHighRisk => match state.sql_modal.status() {
            SqlModalStatus::ConfirmingHigh { input, .. } => input,
            _ => return None,
        },
        InputTarget::SqlModalAnalyzeHighRisk => match state.sql_modal.status() {
            SqlModalStatus::ConfirmingAnalyzeHigh { input, .. } => input,
            _ => return None,
        },
        InputTarget::ResultCellEdit => state.result_interaction.cell_edit().input(),
        InputTarget::ConnectionSetup => state.connection_setup.focused_input()?,
        InputTarget::CommandLine => &state.command_line_input,
        InputTarget::Filter => state.ui.table_picker().filter_input(),
        InputTarget::ErFilter => state.ui.er_picker().filter_input(),
        InputTarget::SettingsErBrowser => state.settings.custom_er_browser(),
        InputTarget::QueryHistoryFilter => state.query_history_picker.filter_input(),
        InputTarget::JsonSearch => state.json_detail.search().input(),
        InputTarget::CellDetailSearch => state.cell_detail.search().input(),
        InputTarget::HelpFilter => state.ui.help().filter(),
    };
    Some((input.content(), input.cursor()))
}

fn filter_selection(step: Step) -> usize {
    if step.edits_text() {
        0
    } else {
        SELECTION_BEFORE_EDIT
    }
}

fn json_search_matches(step: Step) -> &'static [usize] {
    match step {
        Step::Insert(_) | Step::Backspace | Step::Delete | Step::Yank => &[],
        Step::KillToLineStart => &[14],
        _ => &[11],
    }
}

fn cell_detail_search_matches(step: Step) -> &'static [usize] {
    match step {
        Step::Insert(_) | Step::Backspace | Step::Delete => &[],
        _ => &[0],
    }
}

fn assert_side_effects(state: &AppState, target: InputTarget, step: Step, context: &str) {
    match target {
        InputTarget::SqlModal => {
            let status = if step.edits_text() {
                SqlModalStatus::Editing
            } else {
                SqlModalStatus::Normal
            };
            assert_eq!(state.sql_modal.status(), &status, "{context}: status");
            assert_eq!(
                state.sql_modal.completion_debounce().is_some(),
                step.edits_text(),
                "{context}: completion scheduled"
            );
        }
        InputTarget::Filter => assert_eq!(
            state.ui.table_picker().selected(),
            filter_selection(step),
            "{context}: selection"
        ),
        InputTarget::ErFilter => assert_eq!(
            state.ui.er_picker().selected(),
            filter_selection(step),
            "{context}: selection"
        ),
        InputTarget::QueryHistoryFilter => assert_eq!(
            state.query_history_picker.selected(),
            filter_selection(step),
            "{context}: selection"
        ),
        InputTarget::JsonEdit => assert_eq!(
            state.json_detail.validation_error().is_some(),
            matches!(step, Step::Insert('Z') | Step::Yank),
            "{context}: validation error"
        ),
        InputTarget::JsonSearch => assert_eq!(
            state.json_detail.search().matches(),
            json_search_matches(step),
            "{context}: matches"
        ),
        InputTarget::CellDetailSearch => assert_eq!(
            state.cell_detail.search().matches(),
            cell_detail_search_matches(step),
            "{context}: matches"
        ),
        InputTarget::SqlModalHighRisk
        | InputTarget::SqlModalAnalyzeHighRisk
        | InputTarget::ResultCellEdit
        | InputTarget::ConnectionSetup
        | InputTarget::CommandLine
        | InputTarget::SettingsErBrowser
        | InputTarget::HelpFilter => {}
    }
}

mod paste {
    use super::*;
    use crate::model::shared::settings::SettingsSection;
    use crate::ports::inbound::InputEvent;
    use crate::update::input::handle_event;

    const PASTED: &str = "zz";

    fn paste(state: &mut AppState) -> Action {
        let action = handle_event(InputEvent::Paste(PASTED.to_string()), state);
        dispatch(state, action.clone());
        action
    }

    fn editing_state(target: InputTarget) -> AppState {
        let mut state = state_for(target);
        if target == InputTarget::SqlModal {
            dispatch(&mut state, Action::SqlModalEnterInsert);
        }
        state
    }

    fn inserted_at_cursor(content: &str, cursor: usize) -> String {
        let mut expected: String = content.chars().take(cursor).collect();
        expected.push_str(PASTED);
        expected.extend(content.chars().skip(cursor));
        expected
    }

    #[test]
    fn editing_inputs_receive_pasted_text_at_cursor() {
        for target in TARGETS {
            let mut state = editing_state(target);
            let (content, cursor) =
                edited_text(&state, target).unwrap_or_else(|| panic!("{target:?} has no input"));
            let content = content.to_string();

            let action = paste(&mut state);

            assert!(
                matches!(action, Action::Paste { target: actual, .. } if actual == target),
                "{target:?} produced {action:?}"
            );
            assert_eq!(
                edited_text(&state, target).map(|(content, _)| content.to_string()),
                Some(inserted_at_cursor(&content, cursor)),
                "{target:?}"
            );
        }
    }

    fn viewing_states() -> Vec<(&'static str, AppState)> {
        let mut json_detail = state_for(InputTarget::JsonSearch);
        dispatch(
            &mut json_detail,
            Action::DetailSearch {
                target: DetailTarget::JsonDetail,
                step: SearchStep::Exit,
            },
        );
        let mut cell_detail = state_for(InputTarget::CellDetailSearch);
        dispatch(
            &mut cell_detail,
            Action::DetailSearch {
                target: DetailTarget::CellDetail,
                step: SearchStep::Exit,
            },
        );
        let mut help = connected_state();
        dispatch(&mut help, Action::ToggleHelp);
        let mut settings = connected_state();
        dispatch(&mut settings, Action::OpenModal(ModalKind::Settings));
        while settings.settings.section() != SettingsSection::ErDiagram {
            dispatch(&mut settings, Action::SettingsNextSection);
        }
        let mut connection_setup = state_for(InputTarget::ConnectionSetup);
        while !connection_setup.connection_setup.has_open_dropdown() {
            dispatch(&mut connection_setup, Action::ConnectionSetupPrevField);
            dispatch(&mut connection_setup, Action::ConnectionSetupToggleDropdown);
        }
        let mut palette = connected_state();
        dispatch(&mut palette, Action::OpenModal(ModalKind::CommandPalette));

        vec![
            ("browse", connected_state()),
            ("SQL modal normal", state_for(InputTarget::SqlModal)),
            ("JSON detail", json_detail),
            ("cell detail", cell_detail),
            ("help", help),
            ("settings", settings),
            ("connection setup dropdown", connection_setup),
            ("command palette", palette),
        ]
    }

    #[test]
    fn viewing_states_ignore_paste() {
        for (label, mut state) in viewing_states() {
            let mode = state.input_mode();
            let edited: Vec<_> = TARGETS
                .iter()
                .map(|&target| edited_text(&state, target).map(|(content, _)| content.to_string()))
                .collect();

            let action = paste(&mut state);

            assert!(action.is_none(), "{label} produced {action:?}");
            assert_eq!(state.input_mode(), mode, "{label}");
            let after: Vec<_> = TARGETS
                .iter()
                .map(|&target| edited_text(&state, target).map(|(content, _)| content.to_string()))
                .collect();
            assert_eq!(after, edited, "{label}");
        }
    }

    #[test]
    fn detail_viewing_keeps_search_matches() {
        for (label, mut state) in viewing_states() {
            let json_matches = state.json_detail.search().matches().to_vec();
            let cell_matches = state.cell_detail.search().matches().to_vec();

            paste(&mut state);

            assert_eq!(
                state.json_detail.search().matches(),
                json_matches,
                "{label}"
            );
            assert_eq!(
                state.cell_detail.search().matches(),
                cell_matches,
                "{label}"
            );
        }
    }
}

#[test]
fn text_edits_keep_per_target_behavior() {
    for target in TARGETS {
        for case in cases_for(target) {
            let state = state_after(target, case.step);
            let context = format!("{target:?} {:?}", case.step);
            let (content, cursor) = edited_text(&state, target)
                .unwrap_or_else(|| panic!("{context}: input is no longer open"));

            assert_eq!(content, case.content, "{context}: content");
            assert_eq!(cursor, case.cursor, "{context}: cursor");
            assert_eq!(state.kill_buffer(), case.kill, "{context}: kill buffer");
            assert_side_effects(&state, target, case.step, &context);
        }
    }
}

#[rstest::rstest]
#[case::er_filter(InputTarget::ErFilter, TextKillDirection::ToLineStart, "ab cd")]
#[case::query_history(
    InputTarget::QueryHistoryFilter,
    TextKillDirection::ToLineStart,
    "ab cd"
)]
#[case::settings_browser(
    InputTarget::SettingsErBrowser,
    TextKillDirection::ToLineStart,
    "ab cd"
)]
#[case::cell_editor(InputTarget::ResultCellEdit, TextKillDirection::ToLineEnd, "cd")]
fn kill_then_yank_restores_text(
    #[case] target: InputTarget,
    #[case] direction: TextKillDirection,
    #[case] killed: &str,
) {
    let mut state = state_for(target);
    if matches!(direction, TextKillDirection::ToLineStart) {
        dispatch(
            &mut state,
            Action::TextMoveCursor {
                target,
                direction: CursorMove::End,
            },
        );
    }
    dispatch(&mut state, Action::TextKill { target, direction });
    if matches!(direction, TextKillDirection::ToLineStart) {
        assert_eq!(edited_text(&state, target), Some(("", 0)));
    }
    dispatch(&mut state, Action::TextYank { target });

    assert_eq!(edited_text(&state, target).unwrap().0, "ab cd");
    assert_eq!(state.kill_buffer(), Some(killed));
}
