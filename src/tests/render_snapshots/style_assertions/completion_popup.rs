use super::*;

#[test]
fn sql_completion_popup_uses_injected_theme_styles() {
    let mut state = postgres_connected_state();
    let now = test_instant();
    let mut terminal = create_test_terminal();
    let theme = ThemePalette {
        component: ComponentTokens {
            modal: ModalTokens {
                border: Color::Rgb(0xdd, 0x44, 0x11),
                ..DEFAULT_THEME.component.modal
            },
            editor: EditorTokens {
                completion_selected_bg: Color::Rgb(0x22, 0x66, 0x33),
                ..DEFAULT_THEME.component.editor
            },
            ..DEFAULT_THEME.component
        },
        ..DEFAULT_THEME
    };

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content("SELECT ".to_string());
    state.sql_modal.enter_editing();
    let candidates = vec![
        CompletionCandidate {
            text: "users".into(),
            kind: CompletionKind::Table,
            score: 100,
        },
        CompletionCandidate {
            text: "posts".into(),
            kind: CompletionKind::Table,
            score: 90,
        },
    ];
    state
        .sql_modal
        .apply_completion_update(&candidates, 7, true);

    let buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);

    let has_selected_completion = has_cell(&buffer, |cell| {
        cell.bg == theme.component.editor.completion_selected_bg
    });
    let has_completion_border = has_cell(&buffer, |cell| {
        cell.symbol() == "┌" && cell.fg == theme.component.modal.border
    });

    assert!(
        has_completion_border,
        "Expected anchored completion popup border to use injected modal border color"
    );
    assert!(
        has_selected_completion,
        "Expected completion popup selection to use injected selected background"
    );
}
