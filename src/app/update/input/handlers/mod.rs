mod cell_detail;
mod connections;
mod editors;
mod interaction;
mod jsonb;
mod normal;
mod overlays;
mod pickers;
mod row_detail;
mod sql_modal;

use crate::model::app_state::AppState;
use crate::model::shared::input_mode::InputMode;
use crate::policy::FeaturePolicy;
use crate::ports::inbound::{InputEvent, KeyCombo};
use crate::update::action::Action;
use crate::update::input::keybindings::readline_action_for;
use interaction::{InputInteraction, resolve_input_interaction};

pub fn handle_event(event: InputEvent, state: &AppState) -> Action {
    match event {
        InputEvent::Init => Action::Render,
        InputEvent::Resize(w, h) => Action::Resize(w, h),
        InputEvent::Key(combo) => handle_key_event(combo, state),
        InputEvent::Paste(text) => handle_paste_event(text, state),
    }
}

fn handle_paste_event(text: String, state: &AppState) -> Action {
    let action = Action::Paste(text);
    let feature_policy = FeaturePolicy::new(state.session.active_engine_feature_profile());
    if !feature_policy.is_enabled(action.feature_requirement_for_state(state)) {
        return Action::None;
    }

    match state.input_mode() {
        InputMode::TablePicker
        | InputMode::ErTablePicker
        | InputMode::CommandLine
        | InputMode::CellEdit
        | InputMode::ConnectionSetup
        | InputMode::SqlModal
        | InputMode::QueryHistoryPicker
        | InputMode::JsonbEdit
        | InputMode::JsonbDetail
        | InputMode::CellDetail => action,
        _ => Action::None,
    }
}

fn handle_key_event(combo: KeyCombo, state: &AppState) -> Action {
    let feature_policy = FeaturePolicy::new(state.session.active_engine_feature_profile());
    let interaction = resolve_input_interaction(state);
    match interaction {
        InputInteraction::FormEditing(target) => {
            if let Some(action) = readline_action_for(&combo, target) {
                return action;
            }
        }
        InputInteraction::VimEditing(target) if readline_action_for(&combo, target).is_some() => {
            return Action::None;
        }
        InputInteraction::Viewing | InputInteraction::VimEditing(_) => {}
    }
    match state.input_mode() {
        InputMode::Normal => normal::handle_normal_mode(combo, state),
        InputMode::CommandLine => {
            editors::handle_command_line_mode_with_policy(combo, &feature_policy)
        }
        InputMode::CellEdit => editors::handle_cell_edit_keys(combo),
        InputMode::TablePicker => pickers::handle_table_picker_keys(combo),
        InputMode::CommandPalette => pickers::handle_command_palette_keys(combo),
        InputMode::Settings => pickers::handle_settings_keys(combo, state),
        InputMode::Help => {
            overlays::handle_help_keys_with_policy(combo, interaction, &feature_policy)
        }
        InputMode::SqlModal => {
            let completion_visible = state.sql_modal.completion().visible
                && !state.sql_modal.completion().candidates.is_empty();
            sql_modal::handle_sql_modal_keys_with_feature_policy(
                combo,
                completion_visible,
                state.sql_modal.status(),
                state
                    .session
                    .active_engine_feature_profile()
                    .normalize_sql_modal_tab(state.sql_modal.active_tab()),
                state.ui.key_sequence().pending_prefix(),
                state.settings.saved_keymap_preset(),
                &feature_policy,
            )
        }
        InputMode::ConnectionSetup => connections::handle_connection_setup_keys(combo, state),
        InputMode::ConnectionError => connections::handle_connection_error_keys(combo),
        InputMode::ConfirmDialog => overlays::handle_confirm_dialog_keys(combo),
        InputMode::SqliteDiagnostics => {
            overlays::handle_sqlite_diagnostics_keys_with_policy(combo, &feature_policy)
        }
        InputMode::ConnectionSelector => connections::handle_connection_selector_keys(combo),
        InputMode::ErTablePicker => pickers::handle_er_table_picker_keys(combo, state),
        InputMode::QueryHistoryPicker => pickers::handle_query_history_picker_keys(combo),
        InputMode::JsonbDetail => jsonb::handle_jsonb_detail_keys_with_policy(
            combo,
            interaction,
            state.ui.key_sequence().pending_prefix(),
            &feature_policy,
        ),
        InputMode::JsonbEdit => jsonb::handle_jsonb_edit_keys_with_policy(combo, &feature_policy),
        InputMode::CellDetail => {
            let is_searching = state.cell_detail.search().is_active();
            cell_detail::handle_cell_detail_keys(combo, is_searching)
        }
        InputMode::RowDetail => row_detail::handle_row_detail_keys(combo),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shared::settings::{KeymapPreset, SettingsSection};
    use crate::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel};
    use crate::ports::inbound::Key;
    use crate::update::action::ModalKind;
    use crate::update::action::{
        CursorMove, InputTarget, ScrollAmount, ScrollDirection, ScrollTarget, TextKillDirection,
    };
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    mod mode_dispatch {
        use super::*;
        use crate::domain::{ConnectionId, DatabaseType};
        use crate::model::sql_editor::modal::SqlModalTab;
        use crate::update::test_fixtures;

        fn sqlite_connected_state() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.session.activate_connection_with_dsn(
                &ConnectionId::new(),
                "database",
                DatabaseType::SQLite,
                "sqlite://test.db",
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

            assert!(matches!(result, Action::CellDetailEnterSearch));
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
        SettingsBrowser,
        ConnectionSetup,
        SqlModalHighRisk,
        SqlModalAnalyzeHighRisk,
        JsonbSearch,
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
                state.modal.set_mode(InputMode::ErTablePicker);
                InputTarget::ErFilter
            }
            FormSurface::QueryHistoryFilter => {
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                InputTarget::QueryHistoryFilter
            }
            FormSurface::SettingsBrowser => {
                state.modal.set_mode(InputMode::Settings);
                state.settings.switch_next_section();
                state.settings.switch_next_section();
                assert_eq!(state.settings.section(), SettingsSection::ErDiagram);
                state.settings.start_custom_browser_edit();
                InputTarget::SettingsErBrowser
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
                state.modal.set_mode(InputMode::SqlModal);
                state.sql_modal.begin_confirming_analyze_high(
                    "EXPLAIN DELETE FROM users".to_string(),
                    "users".to_string(),
                );
                InputTarget::SqlModalAnalyzeHighRisk
            }
            FormSurface::JsonbSearch => {
                state.modal.set_mode(InputMode::JsonbDetail);
                state.jsonb_detail.enter_search();
                InputTarget::JsonbSearch
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
    #[case(FormSurface::SettingsBrowser)]
    #[case(FormSurface::ConnectionSetup)]
    #[case(FormSurface::SqlModalHighRisk)]
    #[case(FormSurface::SqlModalAnalyzeHighRisk)]
    #[case(FormSurface::JsonbSearch)]
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

    #[rstest]
    #[case(InputTarget::CommandLine)]
    #[case(InputTarget::ResultCellEdit)]
    #[case(InputTarget::Filter)]
    #[case(InputTarget::ErFilter)]
    #[case(InputTarget::QueryHistoryFilter)]
    #[case(InputTarget::SettingsErBrowser)]
    #[case(InputTarget::ConnectionSetup)]
    #[case(InputTarget::SqlModalHighRisk)]
    #[case(InputTarget::SqlModalAnalyzeHighRisk)]
    #[case(InputTarget::JsonbSearch)]
    #[case(InputTarget::HelpFilter)]
    fn readline_leaves_ctrl_n_and_ctrl_p_for_existing_handlers(#[case] target: InputTarget) {
        for key in ['n', 'p'] {
            assert!(readline_action_for(&KeyCombo::ctrl(Key::Char(key)), target).is_none());
        }
    }

    #[rstest]
    #[case(InputMode::SqlModal)]
    #[case(InputMode::JsonbEdit)]
    fn document_editors_do_not_intercept_readline_shortcuts(#[case] mode: InputMode) {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(mode);
        if mode == InputMode::SqlModal {
            state.sql_modal.enter_editing();
        }

        let readline_keys = [
            KeyCombo::ctrl(Key::Char('a')),
            KeyCombo::ctrl(Key::Char('e')),
            KeyCombo::ctrl(Key::Char('b')),
            KeyCombo::ctrl(Key::Char('f')),
            KeyCombo::ctrl(Key::Char('h')),
            KeyCombo::ctrl(Key::Char('d')),
            KeyCombo::ctrl(Key::Char('k')),
            KeyCombo::ctrl(Key::Char('u')),
            KeyCombo::ctrl(Key::Char('w')),
            KeyCombo::ctrl(Key::Char('y')),
            KeyCombo::alt(Key::Char('b')),
            KeyCombo::alt(Key::Char('f')),
            KeyCombo::alt(Key::Char('d')),
        ];

        for combo in readline_keys {
            let result = handle_key_event(combo, &state);

            assert!(
                matches!(result, Action::None),
                "{combo:?} should be unbound"
            );
        }
    }

    #[rstest]
    #[case(KeymapPreset::Default)]
    #[case(KeymapPreset::Ide)]
    fn sql_editing_ctrl_e_is_unbound_for_each_preset(#[case] preset: KeymapPreset) {
        let mut state = AppState::new("test".to_string());
        state.modal.set_mode(InputMode::SqlModal);
        state.sql_modal.enter_editing();
        state.settings.load_keymap_preset(preset);

        let result = handle_key_event(KeyCombo::ctrl(Key::Char('e')), &state);

        assert!(matches!(result, Action::None));
    }

    #[test]
    fn help_routes_ctrl_u_to_scroll_while_viewing_and_readline_while_editing() {
        let mut viewing = AppState::new("test".to_string());
        viewing.modal.set_mode(InputMode::Help);
        let viewing_action = handle_key_event(KeyCombo::ctrl(Key::Char('u')), &viewing);
        assert!(matches!(
            viewing_action,
            Action::Scroll {
                target: ScrollTarget::Help,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::HalfPage,
            }
        ));

        viewing.ui.help_mut().enter_filter_editing();
        let editing_action = handle_key_event(KeyCombo::ctrl(Key::Char('u')), &viewing);
        assert!(matches!(
            editing_action,
            Action::TextKill {
                target: InputTarget::HelpFilter,
                direction: TextKillDirection::ToLineStart,
            }
        ));
    }

    mod paste_event {
        use super::*;
        use crate::update::test_fixtures;

        fn make_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(mode);
            state
        }

        #[test]
        fn sql_modal_pastes_text() {
            let state = make_state(InputMode::SqlModal);

            let result = handle_paste_event("hello".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "hello"));
        }

        #[test]
        fn table_picker_pastes_text() {
            let state = make_state(InputMode::TablePicker);

            let result = handle_paste_event("world".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "world"));
        }

        #[test]
        fn er_table_picker_pastes_text() {
            let mut state = make_state(InputMode::ErTablePicker);
            test_fixtures::activate_postgres_connection(&mut state, "postgres://localhost/test");

            let result = handle_paste_event("public.users".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "public.users"));
        }

        #[test]
        fn sqlite_ignores_er_table_picker_paste() {
            let mut state = make_state(InputMode::ErTablePicker);
            test_fixtures::activate_sqlite_connection(&mut state, "sqlite://test.db");

            let result = handle_paste_event("public.users".to_string(), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn sqlite_ignores_jsonb_edit_paste() {
            let mut state = make_state(InputMode::JsonbEdit);
            test_fixtures::activate_sqlite_connection(&mut state, "sqlite://test.db");

            let result = handle_paste_event("{}".to_string(), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn query_history_picker_pastes_text() {
            let state = make_state(InputMode::QueryHistoryPicker);

            let result = handle_paste_event("users".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "users"));
        }

        #[test]
        fn cell_detail_pastes_text() {
            let state = make_state(InputMode::CellDetail);

            let result = handle_paste_event("needle".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "needle"));
        }

        #[test]
        fn normal_mode_ignores_paste() {
            let state = make_state(InputMode::Normal);

            let result = handle_paste_event("text".to_string(), &state);

            assert!(matches!(result, Action::None));
        }

        #[test]
        fn help_mode_ignores_paste() {
            let state = make_state(InputMode::Help);

            let result = handle_paste_event("text".to_string(), &state);

            assert!(matches!(result, Action::None));
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
