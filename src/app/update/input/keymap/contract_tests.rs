use super::*;
use crate::domain::{ConnectionId, DatabaseType, QueryResult, QuerySource};
use crate::model::app_state::AppState;
use crate::model::connection::error::test_support;
use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::KeySequenceState;

use crate::model::sql_editor::completion::{CompletionCandidate, CompletionKind};
use crate::model::sql_editor::modal::SqlModalTab;
use crate::policy::write::sql_risk::AcknowledgeReason;
use crate::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel};
use crate::ports::inbound::InputEvent;
use crate::test_support::endpoint;
use crate::update::action::ModalKind;
use crate::update::input::handle_event;
use crate::update::input::keymap::sql_edit_hints;
use std::sync::Arc;

fn check_state(state: &mut AppState) {
    let context = KeyContext::from_state(state);
    let Some(rows) = super::super::scope_rows(&context) else {
        return;
    };
    let preset = context.preset;
    let surfaces = [
        HintSurface::Footer,
        HintSurface::Border,
        HintSurface::Current,
        HintSurface::FooterAction,
        HintSurface::FooterNavigation,
        HintSurface::FooterHelp,
        HintSurface::FooterClose,
        HintSurface::FooterQuit,
    ];
    for (row_index, row) in rows.iter().enumerate() {
        let visible = surfaces.iter().any(|surface| {
            row.surfaces.contains(surface)
                && row
                    .visible
                    .is_none_or(|condition| condition(&context, *surface))
        });
        if !visible {
            continue;
        }
        for (binding_index, binding) in row.bindings.iter().enumerate() {
            if !binding.enabled(&context) {
                continue;
            }
            for trigger in binding.triggers(preset) {
                if winner(rows, &context, *trigger) != Some((row_index, binding_index)) {
                    continue;
                }
                let (combo, sequence) = match *trigger {
                    Trigger::Key(combo) => (combo, None),
                    Trigger::AnyModifiers(key) | Trigger::NoCtrlAlt(key) | Trigger::Repeat(key) => {
                        (KeyCombo::plain(key), None)
                    }
                    Trigger::WithAlt(key) => (KeyCombo::alt(key), None),
                    Trigger::WithCtrl(key) => (KeyCombo::ctrl(key), None),
                    Trigger::Sequence { prefix, second, .. } => (second, Some(prefix)),
                    Trigger::Text | Trigger::AnyText => (KeyCombo::plain(Key::Char('x')), None),
                    Trigger::Command { .. } => continue,
                };
                let previous = state.ui.key_sequence;
                if let Some(prefix) = sequence {
                    if previous.pending_prefix().is_none() {
                        let start = handle_event(
                            InputEvent::Key(KeyCombo::plain(Key::Char(prefix_char(prefix)))),
                            state,
                        );
                        assert!(
                            matches!(start, Action::BeginKeySequence(actual) if actual == prefix),
                            "{}: {trigger:?}",
                            context.label()
                        );
                    }
                    state.ui.key_sequence = KeySequenceState::WaitingSecondKey(prefix);
                }
                let actual = handle_event(InputEvent::Key(combo), state);
                state.ui.key_sequence = previous;
                match &binding.output {
                    Output::Action(expected) => assert_eq!(
                        format!("{actual:?}"),
                        format!("{expected:?}"),
                        "{}: {trigger:?}",
                        context.label()
                    ),
                    Output::Text(target) => assert!(
                        matches!(actual, Action::TextInput { target: actual, ch: 'x' } if actual == *target),
                        "{}: {trigger:?}",
                        context.label()
                    ),
                    Output::Consume => assert!(matches!(actual, Action::None)),
                    Output::Command(_) => unreachable!(),
                }
            }
        }
    }

    let mut unconditional = Vec::new();
    for binding in rows
        .iter()
        .flat_map(|row| &row.bindings)
        .filter(|binding| binding.condition.is_none())
    {
        for trigger in binding.triggers(preset) {
            assert!(
                !unconditional.contains(trigger),
                "{}: duplicate unconditional trigger {trigger:?}",
                context.label()
            );
            unconditional.push(*trigger);
        }
    }
    let accepts_text = rows.iter().flat_map(|row| &row.bindings).any(|binding| {
        binding.enabled(&context)
            && binding
                .triggers(preset)
                .iter()
                .any(|trigger| matches!(trigger, Trigger::Text | Trigger::AnyText))
    });
    if accepts_text {
        for ch in ' '..='~' {
            let explicitly_claimed = rows
                .iter()
                .flat_map(|row| &row.bindings)
                .filter(|binding| binding.enabled(&context))
                .any(|binding| {
                    binding.triggers(preset).iter().any(|trigger| {
                        !matches!(trigger, Trigger::Text | Trigger::AnyText)
                            && matches_combo(*trigger, KeyCombo::plain(Key::Char(ch)))
                    })
                });
            if explicitly_claimed {
                assert!(
                    matches!(
                        (context.mode(), ch),
                        (InputMode::CellEdit, ':') | (InputMode::ErTablePicker, ' ')
                    ),
                    "{} claims plain {ch:?}",
                    context.label()
                );
            }
        }
    }
}

fn state(engine: DatabaseType, preset: KeymapPreset, mode: InputMode) -> AppState {
    let mut state = AppState::new("test".to_string());
    state.session.activate_connection(
        &ConnectionId::new(),
        "test",
        &endpoint::for_engine(engine, "test://database"),
    );
    state.settings.load_keymap_preset(preset);
    state.modal.set_mode(mode);
    state
}

#[test]
fn sql_history_hint_and_input_follow_availability() {
    let mut sql = state(
        DatabaseType::PostgreSQL,
        KeymapPreset::Default,
        InputMode::SqlModal,
    );
    sql.sql_modal.enter_editing();
    let key = InputEvent::Key(KeyCombo::ctrl(Key::Char('o')));
    assert!(
        sql_edit_hints(HintSurface::Border, &KeyContext::from_state(&sql))
            .iter()
            .any(|hint| hint.short == "History")
    );
    assert!(matches!(
        handle_event(key.clone(), &sql),
        Action::OpenModal(ModalKind::QueryHistoryPicker)
    ));

    sql.sql_modal.apply_completion_update(
        &[CompletionCandidate {
            text: "users".to_string(),
            kind: CompletionKind::Table,
            score: 1,
        }],
        0,
        true,
    );
    assert!(
        !sql_edit_hints(HintSurface::Border, &KeyContext::from_state(&sql))
            .iter()
            .any(|hint| hint.short == "History")
    );
    assert!(matches!(handle_event(key.clone(), &sql), Action::None));

    sql.sql_modal.dismiss_completion();
    let _ = sql.query.begin_running(std::time::Instant::now());
    assert!(
        !sql_edit_hints(HintSurface::Border, &KeyContext::from_state(&sql))
            .iter()
            .any(|hint| hint.short == "History")
    );
    assert!(matches!(handle_event(key.clone(), &sql), Action::None));
    sql.query.mark_idle();
    let _ = sql.session.begin_mysql_connection_probe(
        &ConnectionId::new(),
        "mysql",
        &endpoint::mysql("mysql://localhost/test", Some("test")),
    );
    assert!(
        !sql_edit_hints(HintSurface::Border, &KeyContext::from_state(&sql))
            .iter()
            .any(|hint| hint.short == "History")
    );
    assert!(matches!(handle_event(key.clone(), &sql), Action::None));
    sql.session.clear_connection();
    assert!(matches!(handle_event(key, &sql), Action::None));
}

#[test]
fn visible_bindings_match_event_entry_in_representative_states() {
    for engine in [
        DatabaseType::PostgreSQL,
        DatabaseType::MySQL,
        DatabaseType::SQLite,
    ] {
        for preset in [KeymapPreset::Default, KeymapPreset::Ide] {
            for mode in [
                InputMode::Normal,
                InputMode::CommandLine,
                InputMode::CellEdit,
                InputMode::TablePicker,
                InputMode::CommandPalette,
                InputMode::Settings,
                InputMode::Help,
                InputMode::SqlModal,
                InputMode::ConnectionSetup,
                InputMode::ConnectionError,
                InputMode::SqliteDiagnostics,
                InputMode::ConfirmDialog,
                InputMode::ConnectionSelector,
                InputMode::ErTablePicker,
                InputMode::QueryHistoryPicker,
                InputMode::JsonDetail,
                InputMode::JsonEdit,
                InputMode::CellDetail,
                InputMode::RowDetail,
            ] {
                let mut state = state(engine, preset, mode);
                check_state(&mut state);
                match mode {
                    InputMode::Normal => {
                        state.focus_pane(FocusedPane::Inspector);
                        check_state(&mut state);
                        state.focus_pane(FocusedPane::Result);
                        state
                            .query
                            .set_current_result(Arc::new(QueryResult::success(
                                "SELECT 1".to_string(),
                                vec!["value".to_string()],
                                vec![vec!["1".to_string()]],
                                1,
                                QuerySource::Preview,
                            )));
                        check_state(&mut state);
                        state.result_interaction.activate_cell(0, 0);
                        check_state(&mut state);
                        state.result_interaction.start_yank_operator();
                        check_state(&mut state);
                        state.result_interaction.start_delete_operator();
                        check_state(&mut state);
                        state.result_interaction.stage_row(0);
                        check_state(&mut state);
                        state
                            .result_interaction
                            .begin_cell_edit(0, 0, "before".to_string());
                        state
                            .result_interaction
                            .replace_cell_edit_draft("after".to_string());
                        check_state(&mut state);
                        state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z);
                        check_state(&mut state);
                        state.ui.key_sequence = KeySequenceState::Idle;
                        state.session.enable_read_only();
                        state.toggle_focus_mode();
                        state.connection_error.set_error(test_support::from_parts(
                            "Connection failed",
                            "Details",
                            false,
                            "boom",
                        ));
                        check_state(&mut state);
                    }
                    InputMode::Help => {
                        state.ui.help_mut().enter_filter_editing();
                        check_state(&mut state);
                    }
                    InputMode::ConnectionSetup => {
                        state.connection_setup.toggle_focused_dropdown();
                        check_state(&mut state);
                        state.connection_setup.toggle_focused_dropdown();
                        state.connection_setup.focus_next_field();
                        check_state(&mut state);
                    }
                    InputMode::JsonDetail => {
                        state.json_detail.enter_search();
                        check_state(&mut state);
                    }
                    InputMode::CellDetail => {
                        state.cell_detail.enter_search();
                        check_state(&mut state);
                    }
                    InputMode::SqlModal => {
                        for tab in [SqlModalTab::Plan, SqlModalTab::Compare] {
                            state.sql_modal.set_active_tab(tab);
                            check_state(&mut state);
                        }
                        state.sql_modal.enter_editing();
                        check_state(&mut state);
                        state.sql_modal.apply_completion_update(
                            &[CompletionCandidate {
                                text: "users".to_string(),
                                kind: CompletionKind::Table,
                                score: 1,
                            }],
                            0,
                            true,
                        );
                        check_state(&mut state);
                        state.sql_modal.begin_confirming_high(
                            AdhocRiskDecision {
                                risk_level: RiskLevel::High,
                                label: "DROP",
                            },
                            "users".to_string(),
                        );
                        check_state(&mut state);
                        state
                            .sql_modal
                            .confirming_high_input_mut()
                            .unwrap()
                            .set_content("users".to_string());
                        check_state(&mut state);
                        state.sql_modal.begin_confirming_analyze_high(
                            "EXPLAIN DELETE FROM users".to_string(),
                            "users".to_string(),
                        );
                        check_state(&mut state);
                        state
                            .sql_modal
                            .confirming_analyze_high_input_mut()
                            .unwrap()
                            .set_content("users".to_string());
                        check_state(&mut state);
                        state.sql_modal.begin_confirming_risk(
                            AcknowledgeReason::UnknownRisk,
                            "DO".to_string(),
                        );
                        check_state(&mut state);
                        state.sql_modal.begin_confirming_analyze_risk(
                            "DO".to_string(),
                            AcknowledgeReason::UnknownRisk,
                        );
                        check_state(&mut state);
                    }
                    _ => {}
                }
            }
        }
    }
}

#[test]
fn conditional_bindings_win_at_the_event_entry() {
    let mut setup = state(
        DatabaseType::PostgreSQL,
        KeymapPreset::Ide,
        InputMode::ConnectionSetup,
    );
    let enter = InputEvent::Key(KeyCombo::plain(Key::Enter));
    let save = InputEvent::Key(KeyCombo::ctrl(Key::Char('s')));
    assert!(matches!(
        handle_event(enter.clone(), &setup),
        Action::ConnectionSetupToggleDropdown
    ));
    assert!(matches!(
        handle_event(save.clone(), &setup),
        Action::ConnectionSetupSave
    ));
    setup.connection_setup.toggle_focused_dropdown();
    assert!(matches!(
        handle_event(enter.clone(), &setup),
        Action::ConnectionSetupDropdownConfirm
    ));
    assert!(matches!(
        handle_event(save, &setup),
        Action::ConnectionSetupSave
    ));

    let mut sql = state(
        DatabaseType::PostgreSQL,
        KeymapPreset::Default,
        InputMode::SqlModal,
    );
    sql.sql_modal.enter_editing();
    sql.sql_modal.apply_completion_update(
        &[CompletionCandidate {
            text: "users".to_string(),
            kind: CompletionKind::Table,
            score: 1,
        }],
        0,
        true,
    );
    assert!(matches!(
        handle_event(enter.clone(), &sql),
        Action::CompletionAccept
    ));
    assert!(matches!(
        handle_event(InputEvent::Key(KeyCombo::plain(Key::Esc)), &sql),
        Action::CompletionDismiss
    ));

    let mut result = state(
        DatabaseType::PostgreSQL,
        KeymapPreset::Default,
        InputMode::Normal,
    );
    result.focus_pane(FocusedPane::Result);
    result.connection_error.set_error(test_support::from_parts(
        "Connection failed",
        "Details",
        false,
        "boom",
    ));
    assert!(matches!(
        handle_event(enter, &result),
        Action::ConfirmSelection
    ));
}

#[test]
fn repeated_operators_and_prefixes_reach_their_state_transitions() {
    use crate::cmd::effect::Effect;
    use crate::update::reducer::reduce;
    use std::time::Instant;

    fn press(state: &mut AppState, key: char) -> Vec<Effect> {
        let action = handle_event(InputEvent::Key(KeyCombo::plain(Key::Char(key))), state);
        reduce(state, action, Instant::now())
    }
    let mut state = state(
        DatabaseType::PostgreSQL,
        KeymapPreset::Default,
        InputMode::Normal,
    );
    state.focus_pane(FocusedPane::Result);
    state
        .query
        .set_current_result(Arc::new(QueryResult::success(
            "SELECT 1".to_string(),
            vec!["value".to_string()],
            vec![vec!["1".to_string()]],
            1,
            QuerySource::Preview,
        )));
    state.result_interaction.activate_cell(0, 0);

    assert!(press(&mut state, 'y').is_empty());
    assert!(state.result_interaction.is_yank_operator_pending());
    assert!(
        matches!(press(&mut state, 'y').as_slice(), [Effect::CopyToClipboard { content, .. }] if content == "1")
    );
    assert!(!state.result_interaction.is_yank_operator_pending());
    press(&mut state, 'y');
    press(&mut state, 'h');
    assert!(!state.result_interaction.is_yank_operator_pending());
    press(&mut state, 'd');
    assert!(state.result_interaction.is_delete_operator_pending());
    press(&mut state, 'd');
    assert!(state.result_interaction.staged_delete_rows().contains(&0));
    assert!(!state.result_interaction.is_delete_operator_pending());
    press(&mut state, 'z');
    assert_eq!(
        state.ui.key_sequence,
        KeySequenceState::WaitingSecondKey(Prefix::Z)
    );
    press(&mut state, 't');
    assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);
}
