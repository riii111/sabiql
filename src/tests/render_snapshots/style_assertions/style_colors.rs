use super::*;
use sabiql_ui::theme::NavigationTokens;

fn contrast_theme() -> ThemePalette {
    ThemePalette {
        component: ComponentTokens {
            navigation: NavigationTokens {
                section_header: Color::Rgb(0x2f, 0xc4, 0xb2),
                scrollbar_active: Color::Rgb(0x2f, 0xc4, 0xb2),
                ..DEFAULT_THEME.component.navigation
            },
            ..DEFAULT_THEME.component
        },
        ..DEFAULT_THEME
    }
}

#[test]
fn pending_draft_cell_uses_orange_fg() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(1, 2);
    state.modal.set_mode(InputMode::Normal);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state
        .result_interaction
        .replace_cell_edit_draft("new@example.com".to_string());

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let draft_cell = has_cell(&buffer, |cell| {
        cell.fg == DEFAULT_THEME.semantic.status.pending
    });
    assert!(
        draft_cell,
        "Expected at least one cell with CELL_DRAFT_PENDING_FG (orange) in the buffer"
    );
}
#[test]
fn active_cell_edit_uses_yellow_fg() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(1, 2);
    state.modal.set_mode(InputMode::CellEdit);
    state
        .result_interaction
        .begin_cell_edit(1, 2, "bob@example.com".to_string());
    state
        .result_interaction
        .replace_cell_edit_draft("new@example.com".to_string());

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let edit_cell = has_cell(&buffer, |cell| {
        cell.fg == DEFAULT_THEME.component.table.cell_edit_fg
    });
    assert!(
        edit_cell,
        "Expected at least one cell with CELL_EDIT_FG (yellow) in the buffer"
    );
}

#[test]
fn staged_delete_row_uses_dark_red_bg() {
    let mut state = table_detail_loaded_state();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    state.ui.set_focused_pane(FocusedPane::Result);
    state.result_interaction.activate_cell(0, 0);
    state.result_interaction.stage_row(1);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let staged_cell = has_cell(&buffer, |cell| {
        cell.bg == DEFAULT_THEME.component.table.staged_delete_bg
    });
    assert!(
        staged_cell,
        "Expected at least one cell with STAGED_DELETE_BG (dark red) in the buffer"
    );
}

#[test]
fn scrim_applies_dim_modifier() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::Help);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let cell = buffer.cell((0, 0)).unwrap();
    assert!(
        cell.modifier.contains(Modifier::DIM),
        "Expected DIM modifier on scrim cell (0,0), got {:?}",
        cell.modifier
    );
}

#[test]
fn result_highlight_respects_injected_now() {
    let mut state = table_detail_loaded_state();
    let now = test_instant();
    let mut terminal = create_test_terminal();

    with_current_result(&mut state);
    // Unfocused so highlight border is distinguishable from focus border
    state.ui.set_focused_pane(FocusedPane::Explorer);

    let highlight_until = now + Duration::from_millis(500);
    state.query.set_result_highlight(highlight_until);

    // Find the Result pane border by searching for "Result" title with Green fg
    let before = now + Duration::from_millis(100);
    let buf_before = render_and_get_buffer_at(&mut terminal, &mut state, before);

    let has_green_border = has_cell(&buf_before, |cell| {
        cell.fg == DEFAULT_THEME.semantic.surface.highlight_border && cell.symbol() == "─"
    });
    assert!(
        has_green_border,
        "Expected Green border cells when now < highlight_until"
    );

    // now >= highlight_until → no Green border cells
    let after = highlight_until + Duration::from_millis(1);
    let buf_after = render_and_get_buffer_at(&mut terminal, &mut state, after);

    let has_green_border_after = has_cell(&buf_after, |cell| {
        cell.fg == DEFAULT_THEME.semantic.surface.highlight_border && cell.symbol() == "─"
    });
    assert!(
        !has_green_border_after,
        "Expected no Green border cells when now >= highlight_until"
    );
}

#[test]
fn modal_border_uses_theme_color() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::Help);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let (mx, my) = help_modal_origin();
    let cell = buffer.cell((mx, my)).unwrap();
    assert_eq!(
        cell.symbol(),
        "╭",
        "Expected '╭' at modal origin ({}, {}), got '{}'",
        mx,
        my,
        cell.symbol()
    );
    assert_eq!(
        cell.fg, DEFAULT_THEME.component.modal.border,
        "Expected MODAL_BORDER fg on modal border at ({}, {}), got {:?}",
        mx, my, cell.fg
    );
}

#[test]
fn header_status_uses_success_warning_and_error_colors() {
    let mut terminal = create_test_terminal();

    let now = test_instant();
    let mut connected = create_test_state();
    let _ = connected
        .session
        .begin_connecting("postgres://localhost/test");
    connected
        .session
        .mark_connected(Arc::new(fixtures::sample_metadata()));
    let connected_buffer = render_and_get_buffer_at(&mut terminal, &mut connected, now);
    assert_header_status_color(
        &connected_buffer,
        "connected",
        DEFAULT_THEME.semantic.status.success,
    );

    let mut loading = create_test_state();
    let _ = loading
        .session
        .begin_connecting("postgres://localhost/test");
    let loading_buffer = render_and_get_buffer_at(&mut terminal, &mut loading, now);
    assert_header_status_color(
        &loading_buffer,
        "loading...",
        DEFAULT_THEME.semantic.status.warning,
    );

    let mut no_dsn = create_test_state();
    no_dsn.session.clear_connection();
    let no_dsn_buffer = render_and_get_buffer_at(&mut terminal, &mut no_dsn, now);
    assert_header_status_color(
        &no_dsn_buffer,
        "no dsn",
        DEFAULT_THEME.semantic.status.error,
    );
}

#[test]
fn read_only_header_uses_warning_badge_style() {
    let mut state = postgres_connected_state();
    state.session.activate_connection_with_dsn(
        &ConnectionId::new(),
        "test",
        DatabaseType::PostgreSQL,
        "postgresql://localhost/test",
    );
    state.session.enable_read_only();
    let mut terminal = create_test_terminal();

    let buffer = render_and_get_buffer(&mut terminal, &mut state);
    let start = find_row0_text_start(&buffer, "READ-ONLY")
        .expect("Expected READ-ONLY badge text in header");

    for offset in 0.."READ-ONLY".len() {
        let cell = buffer
            .cell((start + offset as u16, 0))
            .expect("READ-ONLY badge cell in row 0");
        assert_eq!(cell.fg, Color::Black);
        assert_eq!(cell.bg, DEFAULT_THEME.semantic.status.warning);
        assert!(cell.modifier.contains(Modifier::BOLD));
    }
}

#[test]
fn help_overlay_uses_section_header_and_scrollbar_colors() {
    let mut state = postgres_connected_state();
    let now = test_instant();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::Help);
    state.ui.help_mut().set_scroll_offset(14);

    let buffer = render_and_get_buffer_at(&mut terminal, &mut state, now);

    let has_section_header = has_cell(&buffer, |cell| {
        cell.symbol() == "▸" && cell.fg == DEFAULT_THEME.component.navigation.section_header
    });
    assert!(
        has_section_header,
        "Expected help overlay section markers to use section_header color"
    );

    let has_active_scrollbar = has_cell(&buffer, |cell| {
        matches!(cell.symbol(), "▲" | "▼" | "┃")
            && cell.fg == DEFAULT_THEME.component.navigation.scrollbar_active
    });
    assert!(
        has_active_scrollbar,
        "Expected help overlay active scrollbar parts to use scrollbar_active color"
    );

    let has_inactive_scrollbar = has_cell(&buffer, |cell| {
        cell.symbol() == "│" && cell.fg == DEFAULT_THEME.component.navigation.scrollbar_inactive
    });
    assert!(
        has_inactive_scrollbar,
        "Expected help overlay scrollbar track to use scrollbar_inactive color"
    );
}

#[test]
fn help_overlay_omits_scrollbars_when_content_fits() {
    let mut state = postgres_connected_state();
    let mut terminal = create_test_terminal_sized(180, 300);

    state.modal.set_mode(InputMode::Help);
    state.ui.set_terminal_width(180);
    state.ui.set_terminal_height(300);

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    // A rendered scrollbar always includes ▲▼ arrows + ┃ thumb (vertical) or
    // a ═ thumb (horizontal), so glyph absence alone proves the bars are gone.
    // The │ track glyph is skipped: hint separators share its color tokens.
    let has_scrollbar_part = has_cell(&buffer, |cell| {
        matches!(cell.symbol(), "▲" | "▼" | "┃" | "═")
    });
    assert!(
        !has_scrollbar_part,
        "Expected no scrollbar glyphs when help content fits the viewport"
    );
}

#[test]
fn test_contrast_theme_applies_help_overlay_navigation_colors() {
    let mut state = postgres_connected_state();
    let now = test_instant();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::Help);
    state.ui.help_mut().set_scroll_offset(14);

    let theme = contrast_theme();
    let buffer = render_and_get_buffer_at_with_theme(&mut terminal, &mut state, now, &theme);

    let has_section_header = has_cell(&buffer, |cell| {
        cell.symbol() == "▸" && cell.fg == theme.component.navigation.section_header
    });
    assert!(
        has_section_header,
        "Expected help overlay to resolve section_header from the contrast theme"
    );

    let has_active_scrollbar = has_cell(&buffer, |cell| {
        matches!(cell.symbol(), "▲" | "▼" | "┃")
            && cell.fg == theme.component.navigation.scrollbar_active
    });
    assert!(
        has_active_scrollbar,
        "Expected help overlay to resolve active scrollbar color from the contrast theme"
    );
}
