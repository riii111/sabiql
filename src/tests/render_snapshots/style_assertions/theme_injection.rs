use super::*;

#[test]
fn injected_palette_changes_shell_modal_and_picker_styles() {
    let mut state = connected_state();
    let now = test_instant();
    let mut terminal = create_test_terminal();
    let theme = ThemePalette {
        semantic: SemanticTokens {
            surface: SurfaceTokens {
                focus_border: Color::Rgb(0x11, 0x88, 0xdd),
                ..DEFAULT_THEME.semantic.surface
            },
            ..DEFAULT_THEME.semantic
        },
        component: ComponentTokens {
            modal: ModalTokens {
                hint: Color::Rgb(0xaa, 0xee, 0x22),
                border: Color::Rgb(0xdd, 0x44, 0x11),
                ..DEFAULT_THEME.component.modal
            },
            editor: EditorTokens {
                completion_selected_bg: Color::Rgb(0x22, 0x66, 0x33),
                ..DEFAULT_THEME.component.editor
            },
            ..DEFAULT_THEME.component
        },
    };

    state.ui.set_focused_pane(FocusedPane::Explorer);
    let shell_buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);
    let has_custom_focus_border = has_cell(&shell_buffer, |cell| {
        cell.symbol() == "─" && cell.fg == theme.semantic.surface.focus_border
    });
    assert!(
        has_custom_focus_border,
        "Expected shell border to use injected focus border color"
    );

    state.modal.set_mode(InputMode::Help);
    let help_buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);
    let (mx, my) = help_modal_origin();
    let modal_corner = help_buffer.cell((mx, my)).unwrap();
    assert_eq!(modal_corner.fg, theme.component.modal.border);
    let has_custom_help_hint = has_cell(&help_buffer, |cell| cell.fg == theme.component.modal.hint);
    assert!(
        has_custom_help_hint,
        "Expected shared modal hint to use injected hint color"
    );

    state.modal.set_mode(InputMode::CommandPalette);
    let picker_buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);
    let has_custom_picker_selection = has_cell(&picker_buffer, |cell| {
        cell.bg == theme.component.editor.completion_selected_bg
    });
    assert!(
        has_custom_picker_selection,
        "Expected picker selection to use injected selected background"
    );

    state.modal.set_mode(InputMode::SqlModal);
    state.sql_modal.enter_normal();
    let sql_buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);
    let has_custom_sql_hint = has_cell(&sql_buffer, |cell| cell.fg == theme.component.modal.hint);
    assert!(
        has_custom_sql_hint,
        "Expected SQL modal hint to use injected hint color"
    );
}
mod settings_theme {
    use super::*;

    #[test]
    fn preview_uses_selected_theme_while_chrome_keeps_current() {
        let mut state = connected_state();
        let mut terminal = create_test_terminal();

        state.settings.open(ThemeId::Default);
        state.settings.select_next();
        state.modal.set_mode(InputMode::Settings);

        let buffer = render_and_get_buffer(&mut terminal, &mut state);
        let has_default_selection = has_cell(&buffer, |cell| {
            cell.symbol() == "L" && cell.bg == DEFAULT_THEME.component.editor.completion_selected_bg
        });
        let has_light_preview_selection = has_cell(&buffer, |cell| {
            cell.symbol() == "S" && cell.bg == LIGHT_THEME.component.editor.completion_selected_bg
        });
        let has_light_preview_focus_border = has_cell(&buffer, |cell| {
            cell.symbol() == "─" && cell.fg == LIGHT_THEME.semantic.surface.focus_border
        });
        let has_chrome_border = has_cell(&buffer, |cell| {
            cell.symbol() == "─" && cell.fg == DEFAULT_THEME.component.modal.border
        });

        assert!(
            has_default_selection,
            "Expected settings list selection to keep current theme colors"
        );
        assert!(
            has_light_preview_selection,
            "Expected settings preview selected row to use selected theme colors"
        );
        assert!(
            has_light_preview_focus_border,
            "Expected settings preview focus border to use selected theme colors"
        );
        assert!(
            has_chrome_border,
            "Expected settings modal chrome to keep current theme colors"
        );
    }
}

#[test]
fn test_contrast_theme_applies_result_pane_table_colors() {
    let mut state = table_detail_loaded_state();
    let now = test_instant();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(0, 0);
    state.result_interaction.stage_row(1);

    let staged_buffer =
        render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &TEST_CONTRAST_THEME);
    let has_staged_delete_bg = has_cell(&staged_buffer, |cell| {
        cell.bg == TEST_CONTRAST_THEME.component.table.staged_delete_bg
    });
    let has_active_cell_bg = has_cell(&staged_buffer, |cell| {
        cell.bg == TEST_CONTRAST_THEME.component.table.result_cell_active_bg
    });

    assert!(
        has_staged_delete_bg,
        "Expected staged delete row to resolve background from TEST_CONTRAST_THEME"
    );
    assert!(
        has_active_cell_bg,
        "Expected active result cell to resolve background from TEST_CONTRAST_THEME"
    );

    state
        .result_interaction
        .begin_cell_edit(0, 0, "1".to_string());
    state
        .result_interaction
        .replace_cell_edit_draft("new@example.com".to_string());

    let draft_buffer =
        render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &TEST_CONTRAST_THEME);
    let has_pending_draft_fg = has_cell(&draft_buffer, |cell| {
        cell.fg == TEST_CONTRAST_THEME.semantic.status.pending
    });

    assert!(
        has_pending_draft_fg,
        "Expected pending draft cell to resolve foreground from TEST_CONTRAST_THEME"
    );
}

#[test]
fn light_theme_applies_shell_and_sql_colors() {
    let mut state = connected_state();
    let now = test_instant();
    let mut terminal = create_test_terminal();

    state.ui.set_focused_pane(FocusedPane::Explorer);
    let shell_buffer =
        render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &LIGHT_THEME);
    let has_light_focus_border = has_cell(&shell_buffer, |cell| {
        cell.symbol() == "─" && cell.fg == LIGHT_THEME.semantic.surface.focus_border
    });
    assert!(
        has_light_focus_border,
        "Expected shell to use light focus border color"
    );

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content("SELECT 42".to_string());
    state.sql_modal.enter_editing();
    let sql_buffer =
        render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &LIGHT_THEME);
    let has_light_keyword = has_cell(&sql_buffer, |cell| {
        cell.symbol() == "S" && cell.fg == LIGHT_THEME.component.syntax.sql_keyword
    });
    let has_light_number = has_cell(&sql_buffer, |cell| {
        cell.symbol() == "4" && cell.fg == LIGHT_THEME.component.syntax.sql_number
    });

    assert!(has_light_keyword, "Expected SQL keyword to use light theme");
    assert!(has_light_number, "Expected SQL number to use light theme");
}
