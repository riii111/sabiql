use std::collections::BTreeMap;
use std::fmt::Write;

use ratatui::{Terminal, backend::TestBackend};
use sabiql_app::catalog::HelpDocument;
use sabiql_app::model::app_state::AppState;
use sabiql_app::model::connection::error::ConnectionErrorInfo;
use sabiql_app::model::connection::setup::ConnectionField;
use sabiql_app::model::shared::focused_pane::FocusedPane;
use sabiql_app::model::shared::help::HelpOrigin;
use sabiql_app::model::shared::input_mode::InputMode;
use sabiql_app::model::shared::key_sequence::{KeySequenceState, Prefix};
use sabiql_app::model::shared::settings::KeymapPreset;
use sabiql_app::model::shared::ui_state::FocusMode;
use sabiql_app::model::sql_editor::completion::{CompletionCandidate, CompletionKind};
use sabiql_app::model::sql_editor::modal::SqlModalTab;
use sabiql_app::policy::write::sql_risk::AcknowledgeReason;
use sabiql_app::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel};
use sabiql_app::ports::inbound::{InputEvent, Key, KeyCombo, Modifiers};
use sabiql_app::ports::outbound::DbOperationError;
use sabiql_app::update::action::Action;
use sabiql_app::update::input::{handle_event, palette::palette_commands};
use sabiql_domain::{ConnectionId, DatabaseType};
use sabiql_ui::shell::footer::Footer;
use sabiql_ui::theme::palette_for;

use super::harness::{create_test_terminal_sized, render_to_string};

fn input_keys() -> Vec<KeyCombo> {
    let mut keys = (' '..='~').map(Key::Char).collect::<Vec<_>>();
    keys.extend([
        Key::Enter,
        Key::Esc,
        Key::Tab,
        Key::BackTab,
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Home,
        Key::End,
        Key::Backspace,
        Key::Delete,
        Key::PageUp,
        Key::PageDown,
        Key::F(1),
        Key::F(5),
    ]);
    keys.into_iter()
        .flat_map(|key| {
            [
                Modifiers::NONE,
                Modifiers::CTRL,
                Modifiers::ALT,
                Modifiers::CTRL_ALT,
                Modifiers::SHIFT,
                Modifiers::CTRL_SHIFT,
            ]
            .into_iter()
            .map(move |modifiers| KeyCombo { key, modifiers })
        })
        .collect()
}

fn state(engine: DatabaseType, preset: KeymapPreset, mode: InputMode) -> AppState {
    let mut state = AppState::new("keymap".to_string());
    state.session.activate_connection_with_dsn(
        &ConnectionId::from_string("keymap-baseline"),
        "keymap",
        engine,
        "test://keymap",
    );
    state.settings.load_keymap_preset(preset);
    state.modal.set_mode(mode);
    state
}

fn describe_combo(combo: KeyCombo) -> String {
    let mut result = String::new();
    if combo.modifiers.contains(Modifiers::CTRL) {
        result.push('^');
    }
    if combo.modifiers.contains(Modifiers::ALT) {
        result.push('⌥');
    }
    if combo.modifiers.contains(Modifiers::SHIFT) {
        result.push('⇧');
    }
    match combo.key {
        Key::Char(ch) => write!(result, "{ch:?}").unwrap(),
        key => write!(result, "{key:?}").unwrap(),
    }
    result
}

fn capture(name: &str, mut state: AppState, output: &mut String) {
    writeln!(output, "## {name}").unwrap();
    let mut actions = BTreeMap::<String, Vec<String>>::new();
    for combo in input_keys() {
        let action = handle_event(InputEvent::Key(combo), &state);
        let description = match action {
            Action::None => continue,
            Action::TextInput { target, .. } => format!("TextInput {{ target: {target:?} }}"),
            _ => format!("{action:?}"),
        };
        actions
            .entry(description)
            .or_default()
            .push(describe_combo(combo));
    }
    for (action, combos) in actions {
        writeln!(output, "{action}: {}", combos.join(", ")).unwrap();
    }

    let theme = palette_for(state.ui.theme_id());
    let mut footer = Terminal::new(TestBackend::new(500, 1)).unwrap();
    footer
        .draw(|frame| Footer::render(frame, frame.area(), &state, Some(0), theme))
        .unwrap();
    let buffer = footer.backend().buffer();
    let footer = (0..buffer.area.width)
        .map(|x| buffer.cell((x, 0)).unwrap().symbol())
        .collect::<String>();
    writeln!(output, "footer: {}", footer.trim_end()).unwrap();

    let mut terminal = create_test_terminal_sized(165, 51);
    let screen = render_to_string(&mut terminal, &mut state);
    for line in screen
        .lines()
        .filter(|line| line.contains('╭') || line.contains('╰'))
    {
        let start = line.find(['╭', '╰']).unwrap();
        let end = line
            .rfind(['╮', '╯'])
            .map_or(line.len(), |i| i + '╯'.len_utf8());
        writeln!(output, "border: {}", &line[start..end]).unwrap();
    }

    let origin = HelpOrigin::from_state(&state);
    state.ui.help_mut().open(origin);
    let help = HelpDocument::from_state(&state);
    if let Some(current) = help.sections().first() {
        for row in current.rows() {
            writeln!(output, "help: {}: {}", row.key(), row.description()).unwrap();
        }
    }
    for command in palette_commands(
        state.settings.saved_keymap_preset(),
        &state.session.active_engine_feature_profile(),
    ) {
        writeln!(
            output,
            "palette: {}: {} => {:?}",
            command.key, command.description, command.action
        )
        .unwrap();
    }
}

fn capture_matrix(engine: DatabaseType, preset: KeymapPreset) -> String {
    let mut output = String::new();
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
        InputMode::ConfirmDialog,
        InputMode::ConnectionSelector,
        InputMode::ErTablePicker,
        InputMode::QueryHistoryPicker,
        InputMode::JsonDetail,
        InputMode::JsonEdit,
        InputMode::CellDetail,
        InputMode::SqliteDiagnostics,
        InputMode::RowDetail,
    ] {
        capture(
            &format!("{mode:?}"),
            state(engine, preset, mode),
            &mut output,
        );
    }
    for pane in [FocusedPane::Inspector, FocusedPane::Result] {
        let mut current = state(engine, preset, InputMode::Normal);
        current.ui.set_focused_pane(pane);
        capture(&format!("Normal {pane:?}"), current, &mut output);
    }
    let mut focused = state(engine, preset, InputMode::Normal);
    focused
        .ui
        .set_focus_mode(FocusMode::focused(FocusedPane::Explorer));
    focused.ui.set_focused_pane(FocusedPane::Result);
    capture("Normal focus", focused, &mut output);

    let mut readonly = state(engine, preset, InputMode::Normal);
    readonly.session.enable_read_only();
    capture("Normal read-only", readonly, &mut output);

    let mut error = state(engine, preset, InputMode::Normal);
    error.ui.set_focused_pane(FocusedPane::Result);
    error
        .connection_error
        .set_error(ConnectionErrorInfo::from_db_operation_error(
            &DbOperationError::Timeout("connection timed out".to_string()),
        ));
    capture("Normal result with connection error", error, &mut output);

    for variant in [
        "active",
        "draft",
        "staged",
        "yank pending",
        "delete pending",
        "prefix",
    ] {
        let mut result = state(engine, preset, InputMode::Normal);
        super::harness::with_current_result(&mut result);
        result.ui.set_focused_pane(FocusedPane::Result);
        result.result_interaction.activate_cell(0, 0);
        match variant {
            "draft" => {
                result
                    .result_interaction
                    .begin_cell_edit(0, 0, "original".to_string());
                result.result_interaction.cell_edit_insert_char('x');
                result.result_interaction.leave_cell_edit();
            }
            "staged" => result.result_interaction.stage_row(0),
            "yank pending" => result.result_interaction.start_yank_operator(),
            "delete pending" => result.result_interaction.start_delete_operator(),
            "prefix" => result
                .ui
                .set_key_sequence(KeySequenceState::WaitingSecondKey(Prefix::Z)),
            _ => {}
        }
        capture(&format!("Normal result {variant}"), result, &mut output);
    }
    let mut dropdown = state(engine, preset, InputMode::ConnectionSetup);
    dropdown.connection_setup.toggle_focused_dropdown();
    capture("ConnectionSetup dropdown", dropdown, &mut output);

    let mut cleartext = state(engine, preset, InputMode::ConnectionSetup);
    cleartext
        .connection_setup
        .set_database_type(DatabaseType::MySQL);
    super::harness::focus_connection_field(&mut cleartext, ConnectionField::CleartextAuth);
    capture("ConnectionSetup cleartext", cleartext, &mut output);

    let mut sql_edit = state(engine, preset, InputMode::SqlModal);
    sql_edit.sql_modal.enter_editing();
    capture("SqlModal editing", sql_edit, &mut output);

    let mut completion = state(engine, preset, InputMode::SqlModal);
    completion.sql_modal.enter_editing();
    completion.sql_modal.apply_completion_update(
        &[CompletionCandidate {
            text: "users".to_string(),
            kind: CompletionKind::Table,
            score: 1,
        }],
        0,
        true,
    );
    capture("SqlModal completion", completion, &mut output);

    for tab in [SqlModalTab::Plan, SqlModalTab::Compare] {
        let mut sql = state(engine, preset, InputMode::SqlModal);
        sql.sql_modal.set_active_tab(tab);
        capture(&format!("SqlModal {tab:?}"), sql, &mut output);
    }
    let mut confirm_high = state(engine, preset, InputMode::SqlModal);
    confirm_high.sql_modal.begin_confirming_high(
        AdhocRiskDecision {
            risk_level: RiskLevel::High,
            label: "DROP",
        },
        "users".to_string(),
    );
    capture("SqlModal confirm high", confirm_high, &mut output);

    let mut confirm_analyze_high = state(engine, preset, InputMode::SqlModal);
    confirm_analyze_high
        .sql_modal
        .begin_confirming_analyze_high(
            "EXPLAIN DELETE FROM users".to_string(),
            "users".to_string(),
        );
    capture(
        "SqlModal confirm analyze high",
        confirm_analyze_high,
        &mut output,
    );

    let mut confirm_risk = state(engine, preset, InputMode::SqlModal);
    confirm_risk
        .sql_modal
        .begin_confirming_risk(AcknowledgeReason::UnknownRisk, "DO".to_string());
    capture("SqlModal confirm risk", confirm_risk, &mut output);

    let mut confirm_analyze_risk = state(engine, preset, InputMode::SqlModal);
    confirm_analyze_risk
        .sql_modal
        .begin_confirming_analyze_risk("DO".to_string(), AcknowledgeReason::UnknownRisk);
    capture(
        "SqlModal confirm analyze risk",
        confirm_analyze_risk,
        &mut output,
    );
    let mut json_search = state(engine, preset, InputMode::JsonDetail);
    json_search.json_detail.enter_search();
    capture("JsonDetail search", json_search, &mut output);

    let mut help_filter = state(engine, preset, InputMode::Help);
    help_filter.ui.help_mut().enter_filter_editing();
    capture("Help filter", help_filter, &mut output);

    let mut settings_edit = state(engine, preset, InputMode::Settings);
    settings_edit.settings.switch_next_section();
    settings_edit.settings.switch_next_section();
    settings_edit.settings.start_custom_browser_edit();
    capture("Settings browser", settings_edit, &mut output);

    let mut cell_search = state(engine, preset, InputMode::CellDetail);
    cell_search.cell_detail.enter_search();
    capture("CellDetail search", cell_search, &mut output);

    output
}

#[test]
fn keymap_state_matrix() {
    for (engine_name, engine) in [
        ("postgres", DatabaseType::PostgreSQL),
        ("mysql", DatabaseType::MySQL),
        ("sqlite", DatabaseType::SQLite),
    ] {
        for (preset_name, preset) in [
            ("default", KeymapPreset::Default),
            ("ide", KeymapPreset::Ide),
        ] {
            insta::assert_snapshot!(
                format!("keymap_{engine_name}_{preset_name}"),
                capture_matrix(engine, preset)
            );
        }
    }
}
