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
use crate::ports::inbound::{InputEvent, KeyCombo};
use crate::services::AppServices;
use crate::update::action::Action;
use interaction::{InputInteraction, resolve_input_interaction};

pub fn handle_event(event: InputEvent, state: &AppState, services: &AppServices) -> Action {
    match event {
        InputEvent::Init => Action::Render,
        InputEvent::Resize(w, h) => Action::Resize(w, h),
        InputEvent::Key(combo) => handle_key_event(combo, state, services),
        InputEvent::Paste(text) => handle_paste_event(text, state),
    }
}

fn handle_paste_event(text: String, state: &AppState) -> Action {
    match state.input_mode() {
        InputMode::TablePicker
        | InputMode::ErTablePicker
        | InputMode::CommandLine
        | InputMode::CellEdit
        | InputMode::ConnectionSetup
        | InputMode::SqlModal
        | InputMode::QueryHistoryPicker
        | InputMode::JsonbEdit
        | InputMode::JsonbDetail => Action::Paste(text),
        _ => Action::None,
    }
}

fn handle_key_event(combo: KeyCombo, state: &AppState, services: &AppServices) -> Action {
    let interaction = resolve_input_interaction(state);
    if let InputInteraction::Editing(target) = interaction
        && let Some(action) = crate::update::input::keybindings::readline_action_for(&combo, target)
    {
        return action;
    }

    match state.input_mode() {
        InputMode::Normal => normal::handle_normal_mode(combo, state),
        InputMode::CommandLine => editors::handle_command_line_mode(combo),
        InputMode::CellEdit => editors::handle_cell_edit_keys(combo),
        InputMode::TablePicker => pickers::handle_table_picker_keys(combo),
        InputMode::CommandPalette => pickers::handle_command_palette_keys(combo),
        InputMode::Settings => pickers::handle_settings_keys(combo, state),
        InputMode::Help => overlays::handle_help_keys(combo, interaction),
        InputMode::SqlModal => {
            let completion_visible = state.sql_modal.completion().visible
                && !state.sql_modal.completion().candidates.is_empty();
            sql_modal::handle_sql_modal_keys_with_prefix(
                combo,
                completion_visible,
                state.sql_modal.status(),
                services
                    .db_capabilities
                    .normalize_sql_modal_tab(state.sql_modal.active_tab()),
                state.ui.key_sequence.pending_prefix(),
                state.settings.saved_keymap_preset(),
            )
        }
        InputMode::ConnectionSetup => connections::handle_connection_setup_keys(combo, state),
        InputMode::ConnectionError => connections::handle_connection_error_keys(combo),
        InputMode::ConfirmDialog => overlays::handle_confirm_dialog_keys(combo),
        InputMode::ConnectionSelector => connections::handle_connection_selector_keys(combo),
        InputMode::ErTablePicker => pickers::handle_er_table_picker_keys(combo, state),
        InputMode::QueryHistoryPicker => pickers::handle_query_history_picker_keys(combo),
        InputMode::JsonbDetail => jsonb::handle_jsonb_detail_keys(
            combo,
            interaction,
            state.ui.key_sequence.pending_prefix(),
        ),
        InputMode::JsonbEdit => jsonb::handle_jsonb_edit_keys(combo),
        InputMode::RowDetail => row_detail::handle_row_detail_keys(combo),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::browse::jsonb_detail::JsonbDetailMode;
    use crate::model::shared::settings::SettingsSection;
    use crate::ports::inbound::Key;
    use crate::update::action::ModalKind;
    use crate::update::action::{
        CursorMove, InputTarget, ScrollAmount, ScrollDirection, ScrollTarget,
    };
    use rstest::rstest;

    fn combo(k: Key) -> KeyCombo {
        KeyCombo::plain(k)
    }

    mod mode_dispatch {
        use super::*;

        fn make_state(mode: InputMode) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.modal.set_mode(mode);
            state
        }

        #[test]
        fn normal_mode_routes_to_normal_handler() {
            let state = make_state(InputMode::Normal);
            let services = AppServices::stub();

            // 'q' in Normal mode should quit
            let result = handle_key_event(combo(Key::Char('q')), &state, &services);

            assert!(matches!(result, Action::Quit));
        }

        #[test]
        fn sql_modal_mode_routes_to_sql_modal_handler() {
            let state = make_state(InputMode::SqlModal);
            let services = AppServices::stub();

            // Esc in SqlModal (Normal mode, the default) should close modal
            let result = handle_key_event(combo(Key::Esc), &state, &services);

            assert!(matches!(result, Action::CloseModal(ModalKind::SqlModal)));
        }

        #[test]
        fn sql_modal_normalizes_unsupported_tab_before_handling_keys() {
            let mut state = make_state(InputMode::SqlModal);
            state
                .sql_modal
                .set_active_tab(crate::model::sql_editor::modal::SqlModalTab::Plan);

            let mut services = AppServices::stub();
            services.db_capabilities = crate::model::shared::db_capabilities::DbCapabilities::new(
                false,
                vec![crate::model::shared::inspector_tab::InspectorTab::Info],
            );

            let result = handle_key_event(combo(Key::Char('i')), &state, &services);

            assert!(matches!(result, Action::SqlModalEnterInsert));
        }
    }

    #[derive(Clone, Copy)]
    enum EditingSurface {
        CommandLine,
        CellEdit,
        TableFilter,
        ErFilter,
        QueryHistoryFilter,
        SettingsBrowser,
        ConnectionSetup,
        SqlEditor,
        SqlModalHighRisk,
        SqlModalAnalyzeHighRisk,
        JsonbEditor,
        JsonbSearch,
        HelpFilter,
    }

    fn editing_state(surface: EditingSurface) -> (AppState, InputTarget) {
        let mut state = AppState::new("test".to_string());
        let target = match surface {
            EditingSurface::CommandLine => {
                state.modal.set_mode(InputMode::CommandLine);
                InputTarget::CommandLine
            }
            EditingSurface::CellEdit => {
                state.modal.set_mode(InputMode::CellEdit);
                InputTarget::ResultCellEdit
            }
            EditingSurface::TableFilter => {
                state.modal.set_mode(InputMode::TablePicker);
                InputTarget::Filter
            }
            EditingSurface::ErFilter => {
                state.modal.set_mode(InputMode::ErTablePicker);
                InputTarget::ErFilter
            }
            EditingSurface::QueryHistoryFilter => {
                state.modal.set_mode(InputMode::QueryHistoryPicker);
                InputTarget::QueryHistoryFilter
            }
            EditingSurface::SettingsBrowser => {
                state.modal.set_mode(InputMode::Settings);
                state.settings.switch_next_section();
                state.settings.switch_next_section();
                assert_eq!(state.settings.section(), SettingsSection::ErDiagram);
                state.settings.start_custom_browser_edit();
                InputTarget::SettingsErBrowser
            }
            EditingSurface::ConnectionSetup => {
                state.modal.set_mode(InputMode::ConnectionSetup);
                InputTarget::ConnectionSetup
            }
            EditingSurface::SqlEditor => {
                state.modal.set_mode(InputMode::SqlModal);
                state.sql_modal.enter_editing();
                InputTarget::SqlModal
            }
            EditingSurface::SqlModalHighRisk => {
                state.modal.set_mode(InputMode::SqlModal);
                state.sql_modal.begin_confirming_high(
                    crate::policy::write::write_guardrails::AdhocRiskDecision {
                        risk_level: crate::policy::write::write_guardrails::RiskLevel::High,
                        label: "DROP",
                    },
                    "users".to_string(),
                );
                InputTarget::SqlModalHighRisk
            }
            EditingSurface::SqlModalAnalyzeHighRisk => {
                state.modal.set_mode(InputMode::SqlModal);
                state.sql_modal.begin_confirming_analyze_high(
                    "EXPLAIN DELETE FROM users".to_string(),
                    "users".to_string(),
                );
                InputTarget::SqlModalAnalyzeHighRisk
            }
            EditingSurface::JsonbEditor => {
                state.modal.set_mode(InputMode::JsonbDetail);
                state.jsonb_detail.set_mode(JsonbDetailMode::Editing);
                InputTarget::JsonbEdit
            }
            EditingSurface::JsonbSearch => {
                state.modal.set_mode(InputMode::JsonbDetail);
                state.jsonb_detail.enter_search();
                InputTarget::JsonbSearch
            }
            EditingSurface::HelpFilter => {
                state.modal.set_mode(InputMode::Help);
                state.ui.help.toggle_filter_editing();
                InputTarget::HelpFilter
            }
        };
        (state, target)
    }

    #[rstest]
    #[case(EditingSurface::CommandLine)]
    #[case(EditingSurface::CellEdit)]
    #[case(EditingSurface::TableFilter)]
    #[case(EditingSurface::ErFilter)]
    #[case(EditingSurface::QueryHistoryFilter)]
    #[case(EditingSurface::SettingsBrowser)]
    #[case(EditingSurface::ConnectionSetup)]
    #[case(EditingSurface::SqlEditor)]
    #[case(EditingSurface::SqlModalHighRisk)]
    #[case(EditingSurface::SqlModalAnalyzeHighRisk)]
    #[case(EditingSurface::JsonbEditor)]
    #[case(EditingSurface::JsonbSearch)]
    #[case(EditingSurface::HelpFilter)]
    fn editing_surfaces_prioritize_readline(#[case] surface: EditingSurface) {
        let (state, target) = editing_state(surface);
        let result = handle_key_event(KeyCombo::ctrl(Key::Char('a')), &state, &AppServices::stub());

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
    #[case(InputTarget::SqlModal)]
    #[case(InputTarget::SqlModalHighRisk)]
    #[case(InputTarget::SqlModalAnalyzeHighRisk)]
    #[case(InputTarget::JsonbEdit)]
    #[case(InputTarget::JsonbSearch)]
    #[case(InputTarget::HelpFilter)]
    fn readline_leaves_ctrl_n_and_ctrl_p_for_existing_handlers(#[case] target: InputTarget) {
        for key in ['n', 'p'] {
            assert!(
                crate::update::input::keybindings::readline_action_for(
                    &KeyCombo::ctrl(Key::Char(key)),
                    target,
                )
                .is_none()
            );
        }
    }

    #[test]
    fn help_routes_scroll_while_viewing_and_readline_while_editing() {
        let mut viewing = AppState::new("test".to_string());
        viewing.modal.set_mode(InputMode::Help);
        let viewing_action = handle_key_event(
            KeyCombo::ctrl(Key::Char('b')),
            &viewing,
            &AppServices::stub(),
        );
        assert!(matches!(
            viewing_action,
            Action::Scroll {
                target: ScrollTarget::Help,
                direction: ScrollDirection::Up,
                amount: ScrollAmount::FullPage,
            }
        ));

        viewing.ui.help.toggle_filter_editing();
        let editing_action = handle_key_event(
            KeyCombo::ctrl(Key::Char('b')),
            &viewing,
            &AppServices::stub(),
        );
        assert!(matches!(
            editing_action,
            Action::TextMoveCursor {
                target: InputTarget::HelpFilter,
                direction: CursorMove::Left,
            }
        ));
    }

    mod paste_event {
        use super::*;

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
            let state = make_state(InputMode::ErTablePicker);

            let result = handle_paste_event("public.users".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "public.users"));
        }

        #[test]
        fn query_history_picker_pastes_text() {
            let state = make_state(InputMode::QueryHistoryPicker);

            let result = handle_paste_event("users".to_string(), &state);

            assert!(matches!(result, Action::Paste(t) if t == "users"));
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
            let services = AppServices::stub();

            let result = handle_event(InputEvent::Init, &state, &services);

            assert!(matches!(result, Action::Render));
        }

        #[test]
        fn resize_maps_to_resize() {
            let state = AppState::new("test".to_string());
            let services = AppServices::stub();

            let result = handle_event(InputEvent::Resize(80, 24), &state, &services);

            assert!(matches!(result, Action::Resize(80, 24)));
        }
    }
}
