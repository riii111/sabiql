mod mouse;

use crate::model::app_state::AppState;
use crate::ports::inbound::{InputEvent, KeyCombo};
use crate::update::action::Action;
use crate::update::input::keymap::{KeyContext, key_action, paste_target};

pub fn handle_event(event: InputEvent, state: &AppState) -> Action {
    match event {
        InputEvent::Init => Action::Render,
        InputEvent::Resize(w, h) => Action::Resize(w, h),
        InputEvent::Key(combo) => handle_key_event(combo, state),
        InputEvent::Mouse(input) => mouse::handle_mouse_event(input, state),
        InputEvent::Paste(text) => paste_target(&KeyContext::from_state(state))
            .map_or(Action::None, |target| Action::Paste { target, text }),
    }
}

fn handle_key_event(combo: KeyCombo, state: &AppState) -> Action {
    key_action(combo, &KeyContext::from_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::input_mode::InputMode;
    use crate::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel};
    use crate::ports::inbound::Key;
    use crate::test_support::endpoint;
    use crate::update::action::DetailTarget;
    use crate::update::action::ModalKind;
    use crate::update::action::SearchStep;
    use crate::update::action::{CursorMove, InputTarget};
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    mod mode_dispatch {
        use super::*;
        use crate::domain::ConnectionId;
        use crate::model::sql_editor::modal::SqlModalTab;
        use crate::update::test_fixtures;

        fn sqlite_connected_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection(
                &ConnectionId::new(),
                "database",
                &endpoint::sqlite("sqlite://test.db"),
            );
            state
        }

        fn combo_ctrl_shift(key: Key) -> KeyCombo {
            KeyCombo::ctrl_shift(key)
        }

        fn make_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(mode);
            state
        }

        #[test]
        fn normal_mode_routes_to_normal_handler() {
            let state = make_state(InputMode::Normal);

            // 'q' in Normal mode should quit
            let result = handle_key_event(combo(Key::Char('q')), &state);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn sql_modal_mode_routes_to_sql_modal_handler() {
            let state = make_state(InputMode::SqlModal);

            // Esc in SqlModal (Normal mode, the default) should close modal
            let result = handle_key_event(combo(Key::Esc), &state);

            assert!(matches!(result, Action::CloseModal(ModalKind::SqlModal)));
        }

        #[test]
        fn sql_modal_normalizes_unsupported_tab_before_handling_keys() {
            let mut state = make_state(InputMode::SqlModal);
            state.sql_modal.set_active_tab(SqlModalTab::Plan);

            let result = handle_key_event(combo(Key::Char('i')), &state);

            assert!(matches!(result, Action::SqlModalEnterInsert));
        }

        #[test]
        fn cell_detail_mode_routes_to_cell_detail_handler() {
            let state = make_state(InputMode::CellDetail);

            let result = handle_key_event(combo(Key::Char('/')), &state);

            assert!(matches!(
                result,
                Action::DetailSearch {
                    target: DetailTarget::CellDetail,
                    step: SearchStep::Enter,
                }
            ));
        }

        #[test]
        fn engine_specific_global_features_follow_the_active_profile() {
            let state = sqlite_connected_state();

            assert!(matches!(
                handle_key_event(combo(Key::Char('e')), &state),
                Action::None
            ));
            assert!(matches!(
                handle_key_event(combo_ctrl_shift(Key::Char('d')), &state),
                Action::OpenModal(ModalKind::SqliteDiagnostics)
            ));

            let mut postgres_state = AppState::new("test".to_string());
            test_fixtures::activate_postgres_connection(
                &mut postgres_state,
                "postgres://localhost/test",
            );
            assert!(matches!(
                handle_key_event(combo_ctrl_shift(Key::Char('d')), &postgres_state),
                Action::None
            ));
        }

        #[test]
        fn postgres_input_keeps_er_diagram_available() {
            let mut state = make_state(InputMode::Normal);
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");

            assert!(matches!(
                handle_key_event(combo(Key::Char('e')), &state),
                Action::OpenModal(ModalKind::ErTablePicker)
            ));
        }
    }

    #[derive(Clone, Copy)]
    enum FormSurface {
        CommandLine,
        CellEdit,
        TableFilter,
        ErFilter,
        QueryHistoryFilter,
        ConnectionSetup,
        SqlModalHighRisk,
        SqlModalAnalyzeHighRisk,
        JsonSearch,
        CellDetailSearch,
        HelpFilter,
    }

    fn form_editing_state(surface: FormSurface) -> (AppState, InputTarget) {
        let mut state = AppState::new("test".to_string());
        let target = match surface {
            FormSurface::CommandLine => {
                state.modal.set_mode(InputMode::CommandLine);
                InputTarget::CommandLine
            }
            FormSurface::CellEdit => {
                state.modal.set_mode(InputMode::CellEdit);
                InputTarget::ResultCellEdit
            }
            FormSurface::TableFilter => {
                state.modal.set_mode(InputMode::TablePicker);
                InputTarget::Filter
            }
            FormSurface::ErFilter => {
                use crate::update::test_fixtures;
                test_fixtures::activate_postgres_connection(
                    &mut state,
                    "postgres://localhost/test",
                );
                state.modal.set_mode(InputMode::ErTablePicker);
                InputTarget::ErFilter
            }
            FormSurface::QueryHistoryFilter => {
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                InputTarget::QueryHistoryFilter
            }
            FormSurface::ConnectionSetup => {
                state.modal.set_mode(InputMode::ConnectionSetup);
                state.connection_setup.focus_next_field();
                InputTarget::ConnectionSetup
            }
            FormSurface::SqlModalHighRisk => {
                state.modal.set_mode(InputMode::SqlModal);
                state.sql_modal.begin_confirming_high(
                    AdhocRiskDecision {
                        risk_level: RiskLevel::High,
                        label: "DROP",
                    },
                    "users".to_string(),
                );
                InputTarget::SqlModalHighRisk
            }
            FormSurface::SqlModalAnalyzeHighRisk => {
                use crate::update::test_fixtures;
                test_fixtures::activate_postgres_connection(
                    &mut state,
                    "postgres://localhost/test",
                );
                state.modal.set_mode(InputMode::SqlModal);
                state.sql_modal.begin_confirming_analyze_high(
                    "EXPLAIN DELETE FROM users".to_string(),
                    "users".to_string(),
                );
                InputTarget::SqlModalAnalyzeHighRisk
            }
            FormSurface::JsonSearch => {
                use crate::update::test_fixtures;
                test_fixtures::activate_postgres_connection(
                    &mut state,
                    "postgres://localhost/test",
                );
                state.modal.set_mode(InputMode::JsonDetail);
                state.json_detail.enter_search();
                InputTarget::JsonSearch
            }
            FormSurface::CellDetailSearch => {
                state.modal.set_mode(InputMode::CellDetail);
                state.cell_detail.enter_search();
                InputTarget::CellDetailSearch
            }
            FormSurface::HelpFilter => {
                state.modal.set_mode(InputMode::Help);
                state.ui.help_mut().enter_filter_editing();
                InputTarget::HelpFilter
            }
        };
        (state, target)
    }

    #[rstest]
    #[case(FormSurface::CommandLine)]
    #[case(FormSurface::CellEdit)]
    #[case(FormSurface::TableFilter)]
    #[case(FormSurface::ErFilter)]
    #[case(FormSurface::QueryHistoryFilter)]
    #[case(FormSurface::ConnectionSetup)]
    #[case(FormSurface::SqlModalHighRisk)]
    #[case(FormSurface::SqlModalAnalyzeHighRisk)]
    #[case(FormSurface::JsonSearch)]
    #[case(FormSurface::CellDetailSearch)]
    #[case(FormSurface::HelpFilter)]
    fn form_editing_surfaces_prioritize_readline(#[case] surface: FormSurface) {
        let (state, target) = form_editing_state(surface);
        let result = handle_key_event(KeyCombo::ctrl(Key::Char('a')), &state);

        assert!(matches!(
            result,
            Action::TextMoveCursor {
                target: actual_target,
                direction: CursorMove::LineStart,
            } if actual_target == target
        ));
    }

    #[test]
    fn unavailable_json_search_keeps_only_exit() {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::JsonDetail);
        state.json_detail.enter_search();

        assert!(matches!(
            handle_key_event(KeyCombo::ctrl(Key::Char('a')), &state),
            Action::None
        ));
        assert!(matches!(
            handle_key_event(KeyCombo::plain(Key::Esc), &state),
            Action::DetailSearch {
                target: DetailTarget::JsonDetail,
                step: SearchStep::Exit,
            }
        ));
    }

    #[test]
    fn json_prefix_completes_or_cancels_and_search_treats_g_as_text() {
        use crate::model::browse::json_detail::JsonDetailState;
        use crate::model::shared::key_sequence::{KeySequenceState, Prefix};
        use crate::update::{reducer::route, test_fixtures};
        use std::time::Instant;

        let mut state = AppState::new("test".to_string());
        test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");
        state.json_detail = JsonDetailState::open(0, 0, "json".to_string(), "[1, 2]".to_string());
        state.modal.set_mode(InputMode::JsonDetail);

        let first = handle_key_event(KeyCombo::plain(Key::Char('g')), &state);
        assert!(matches!(first, Action::BeginKeySequence(Prefix::G)));
        route(&mut state, first, Instant::now());
        assert_eq!(
            state.ui.key_sequence,
            KeySequenceState::WaitingSecondKey(Prefix::G)
        );
        let second = handle_key_event(KeyCombo::plain(Key::Char('g')), &state);
        assert!(matches!(
            second,
            Action::TextMoveCursor {
                target: InputTarget::JsonEdit,
                direction: CursorMove::FirstLine
            }
        ));
        route(&mut state, second, Instant::now());
        assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);

        let first = handle_key_event(KeyCombo::plain(Key::Char('g')), &state);
        route(&mut state, first, Instant::now());
        let cancel = handle_key_event(KeyCombo::ctrl(Key::Char('g')), &state);
        assert!(matches!(cancel, Action::CancelKeySequence));
        route(&mut state, cancel, Instant::now());
        assert_eq!(state.ui.key_sequence, KeySequenceState::Idle);

        state.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::G);
        state.json_detail.enter_search();
        assert!(matches!(
            handle_key_event(KeyCombo::plain(Key::Char('g')), &state),
            Action::TextInput {
                target: InputTarget::JsonSearch,
                ch: 'g'
            }
        ));
    }

    mod paste_event {
        use super::*;
        use crate::update::test_fixtures;

        fn sqlite_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(mode);
            test_fixtures::activate_sqlite_connection(&mut state, "sqlite://test.db");
            state
        }

        #[rstest]
        #[case::er_table_picker(InputMode::ErTablePicker)]
        #[case::json_edit(InputMode::JsonEdit)]
        fn sqlite_ignores_paste_into_unsupported_input(#[case] mode: InputMode) {
            let result = handle_event(InputEvent::Paste("text".to_string()), &sqlite_state(mode));

            assert!(matches!(result, Action::None), "{result:?}");
        }
    }

    mod input_events {
        use super::*;

        #[test]
        fn init_maps_to_render() {
            let state = AppState::new("test".to_string());

            let result = handle_event(InputEvent::Init, &state);

            assert!(matches!(result, Action::Render));
        }

        #[test]
        fn resize_maps_to_resize() {
            let state = AppState::new("test".to_string());

            let result = handle_event(InputEvent::Resize(80, 24), &state);

            assert!(matches!(result, Action::Resize(80, 24)));
        }
    }
}
