use super::*;

#[test]
fn sql_completion_popup_uses_injected_theme_styles() {
    let mut state = connected_state();
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

    let frame_area = Rect::new(0, 0, TEST_WIDTH, TEST_HEIGHT);
    let [modal_area] = Layout::horizontal([Constraint::Percentage(80)])
        .flex(Flex::Center)
        .areas(frame_area);
    let [modal_area] = Layout::vertical([Constraint::Percentage(60)])
        .flex(Flex::Center)
        .areas(modal_area);
    let inner_area = Rect::new(
        modal_area.x + 1,
        modal_area.y + 1,
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    let content_area = Rect {
        x: inner_area.x + 1,
        width: inner_area.width.saturating_sub(2),
        ..inner_area
    };
    let [editor_area, _, _] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(content_area);
    let (cursor_row, cursor_col) = state.sql_modal.editor().cursor_to_position();
    let cursor_col = cursor_col as u16;
    let cursor_row = cursor_row as u16;
    let scroll_row = state.sql_modal.editor().scroll_row() as u16;
    let visible_count = state.sql_modal.completion().candidates.len().min(8) as u16;
    let popup_height = visible_count + 2;
    let popup_width = 45u16.min(modal_area.width);
    let popup_x = if modal_area.width < popup_width {
        modal_area.x
    } else {
        (editor_area.x + cursor_col).min(modal_area.right().saturating_sub(popup_width))
    };
    let visible_row = cursor_row.saturating_sub(scroll_row);
    let cursor_screen_y = editor_area.y + visible_row;
    let popup_y = if cursor_screen_y + 1 + popup_height > modal_area.bottom() {
        cursor_screen_y.saturating_sub(popup_height)
    } else {
        cursor_screen_y + 1
    };

    let has_selected_completion = has_cell(&buffer, |cell| {
        cell.bg == theme.component.editor.completion_selected_bg
    });
    let top_left = buffer.cell((popup_x, popup_y)).unwrap();
    let top_right = buffer.cell((popup_x + popup_width - 1, popup_y)).unwrap();
    let has_completion_border = top_left.symbol() == "┌"
        && top_left.fg == theme.component.modal.border
        && top_right.symbol() == "┐"
        && top_right.fg == theme.component.modal.border;

    assert!(
        has_completion_border,
        "Expected anchored completion popup border to use injected modal border color"
    );
    assert!(
        has_selected_completion,
        "Expected completion popup selection to use injected selected background"
    );
}
