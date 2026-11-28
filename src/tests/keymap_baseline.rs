use std::collections::BTreeMap;
use std::fmt::Write;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use sabiql_app::catalog::HelpDocument;
use sabiql_app::model::app_state::AppState;
use sabiql_app::model::connection::error::ConnectionErrorInfo;
use sabiql_app::model::connection::setup::ConnectionField;
use sabiql_app::model::shared::focused_pane::FocusedPane;
use sabiql_app::model::shared::input_mode::InputMode;
use sabiql_app::model::shared::key_sequence::{KeySequenceState, Prefix};
use sabiql_app::model::shared::settings::KeymapPreset;
use sabiql_app::model::shared::text_input::TextEdit;

use sabiql_app::model::sql_editor::completion::{CompletionCandidate, CompletionKind};
use sabiql_app::model::sql_editor::modal::SqlModalTab;
use sabiql_app::policy::write::sql_risk::AcknowledgeReason;
use sabiql_app::policy::write::write_guardrails::{AdhocRiskDecision, RiskLevel};
use sabiql_app::ports::inbound::{InputEvent, Key, KeyCombo, Modifiers};
use sabiql_app::ports::outbound::DbOperationError;
use sabiql_app::update::action::Action;
use sabiql_app::update::input::handle_event;
use sabiql_app::update::input::keymap::{KeyContext, palette_available};
use sabiql_domain::test_support::endpoint;
use sabiql_domain::{ConnectionId, DatabaseType};
use sabiql_ui::event::key_translator::translate;
use sabiql_ui::shell::footer::Footer;
use sabiql_ui::theme::palette_for;

fn input_keys() -> Vec<KeyCombo> {
    (' '..='~')
        .map(Key::Char)
        .chain([
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
        ])
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
    state.session.activate_connection(
        &ConnectionId::from_string("keymap-baseline"),
        "keymap",
        &endpoint::for_engine(engine, "test://keymap"),
    );
    state.settings.load_keymap_preset(preset);
    if mode == InputMode::CommandPalette {
        super::harness::open_command_palette(&mut state);
    } else {
        state.modal.set_mode(mode);
    }
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

fn capture(name: &str, mut state: AppState, palettes: &mut BTreeMap<String, String>) -> String {
    let mut output = String::new();
    writeln!(output, "## {name}").unwrap();
    let mut actions = BTreeMap::<String, Vec<String>>::new();
    for combo in input_keys() {
        let normalized = if combo == KeyCombo::ctrl(Key::Char('D')) {
            translate(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::CONTROL))
        } else {
            combo
        };
        let action = handle_event(InputEvent::Key(normalized), &state);
        let description = match action {
            Action::None => continue,
            Action::TextInput { target, ch } => {
                if let Key::Char(input) = normalized.key {
                    assert_eq!(ch, input, "{name}: {normalized:?}");
                }
                format!("TextInput {{ target: {target:?} }}")
            }
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

    let theme = palette_for(state.ui.theme_id);
    let mut footer = Terminal::new(TestBackend::new(500, 1)).unwrap();
    footer
        .draw(|frame| Footer::render(frame, frame.area(), &state, Some(0), theme))
        .unwrap();
    let buffer = footer.backend().buffer();
    let footer = (0..buffer.area.width)
        .map(|x| buffer.cell((x, 0)).unwrap().symbol())
        .collect::<String>();
    writeln!(output, "footer: {}", footer.trim_end()).unwrap();

    let origin = KeyContext::from_state(&state);
    state.ui.help_mut().open(origin);
    let help = HelpDocument::from_state(&state);
    if let Some(current) = help.sections().first() {
        for row in current.rows() {
            writeln!(output, "help: {}: {}", row.key(), row.description()).unwrap();
        }
    }
    let mut palette = String::new();
    for command in palette_available(&KeyContext::for_browse_state(&state)) {
        writeln!(
            palette,
            "palette: {}: {} => {:?}",
            command.key, command.description, command.action
        )
        .unwrap();
    }
    if let Some(first_section) = palettes.get(&palette) {
        writeln!(output, "palette: see {first_section}").unwrap();
    } else {
        output.push_str(&palette);
        palettes.insert(palette, name.to_string());
    }
    output
}

fn capture_matrix(engine: DatabaseType, preset: KeymapPreset) -> Vec<String> {
    let mut sections = Vec::new();
    let mut palettes = BTreeMap::new();
    let mut record = |name: &str, state| sections.push(capture(name, state, &mut palettes));
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
        record(&format!("{mode:?}"), state(engine, preset, mode));
    }
    for pane in [FocusedPane::Inspector, FocusedPane::Result] {
        let mut current = state(engine, preset, InputMode::Normal);
        current.focus_pane(pane);
        record(&format!("Normal {pane:?}"), current);
    }
    let mut focused = state(engine, preset, InputMode::Normal);
    focused.toggle_focus_mode();
    record("Normal focus", focused);

    let mut readonly = state(engine, preset, InputMode::Normal);
    readonly.session.enable_read_only();
    record("Normal read-only", readonly);

    let mut error = state(engine, preset, InputMode::Normal);
    error.focus_pane(FocusedPane::Result);
    error
        .connection_error
        .set_error(ConnectionErrorInfo::from_db_operation_error(
            &DbOperationError::Timeout("connection timed out".to_string()),
        ));
    record("Normal result with connection error", error);

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
        result.focus_pane(FocusedPane::Result);
        result.result_interaction.activate_cell(0, 0);
        match variant {
            "draft" => {
                result
                    .result_interaction
                    .begin_cell_edit(0, 0, "original".to_string());
                result
                    .result_interaction
                    .edit_cell_text(TextEdit::Insert('x'), None);
                result.result_interaction.leave_cell_edit();
            }
            "staged" => result.result_interaction.stage_row(0),
            "yank pending" => result.result_interaction.start_yank_operator(),
            "delete pending" => result.result_interaction.start_delete_operator(),
            "prefix" => result.ui.key_sequence = KeySequenceState::WaitingSecondKey(Prefix::Z),
            _ => {}
        }
        record(&format!("Normal result {variant}"), result);
    }
    let mut dropdown = state(engine, preset, InputMode::ConnectionSetup);
    dropdown.connection_setup.toggle_focused_dropdown();
    record("ConnectionSetup dropdown", dropdown);

    let mut cleartext = state(engine, preset, InputMode::ConnectionSetup);
    cleartext
        .connection_setup
        .set_database_type(DatabaseType::MySQL);
    super::harness::focus_connection_field(&mut cleartext, ConnectionField::CleartextAuth);
    record("ConnectionSetup cleartext", cleartext);

    let mut sql_edit = state(engine, preset, InputMode::SqlModal);
    sql_edit.sql_modal.enter_editing();
    record("SqlModal editing", sql_edit);

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
    record("SqlModal completion", completion);

    for tab in [SqlModalTab::Plan, SqlModalTab::Compare] {
        let mut sql = state(engine, preset, InputMode::SqlModal);
        sql.sql_modal.set_active_tab(tab);
        record(&format!("SqlModal {tab:?}"), sql);
    }
    let mut confirm_high = state(engine, preset, InputMode::SqlModal);
    confirm_high.sql_modal.begin_confirming_high(
        AdhocRiskDecision {
            risk_level: RiskLevel::High,
            label: "DROP",
        },
        "users".to_string(),
    );
    record("SqlModal confirm high", confirm_high);

    let mut confirm_analyze_high = state(engine, preset, InputMode::SqlModal);
    confirm_analyze_high
        .sql_modal
        .begin_confirming_analyze_high(
            "EXPLAIN DELETE FROM users".to_string(),
            "users".to_string(),
        );
    record("SqlModal confirm analyze high", confirm_analyze_high);

    let mut confirm_risk = state(engine, preset, InputMode::SqlModal);
    confirm_risk
        .sql_modal
        .begin_confirming_risk(AcknowledgeReason::UnknownRisk, "DO".to_string());
    record("SqlModal confirm risk", confirm_risk);

    let mut confirm_analyze_risk = state(engine, preset, InputMode::SqlModal);
    confirm_analyze_risk
        .sql_modal
        .begin_confirming_analyze_risk("DO".to_string(), AcknowledgeReason::UnknownRisk);
    record("SqlModal confirm analyze risk", confirm_analyze_risk);
    let mut json_search = state(engine, preset, InputMode::JsonDetail);
    json_search.json_detail.enter_search();
    record("JsonDetail search", json_search);

    let mut help_filter = state(engine, preset, InputMode::Help);
    help_filter.ui.help_mut().enter_filter_editing();
    record("Help filter", help_filter);

    let mut cell_search = state(engine, preset, InputMode::CellDetail);
    cell_search.cell_detail.enter_search();
    record("CellDetail search", cell_search);

    sections
}

#[test]
fn keymap_state_matrix() {
    insta::allow_duplicates! {
        for (snapshot_engine, engine) in [
            ("postgres", DatabaseType::PostgreSQL),
            ("postgres", DatabaseType::MySQL),
            ("sqlite", DatabaseType::SQLite),
        ] {
            let default = capture_matrix(engine, KeymapPreset::Default);
            let mut ide = capture_matrix(engine, KeymapPreset::Ide);
            ide.retain(|section| !default.contains(section));

            insta::assert_snapshot!(format!("keymap_{snapshot_engine}_default"), default.concat());
            insta::assert_snapshot!(format!("keymap_{snapshot_engine}_ide"), ide.concat());
        }
    }
}
