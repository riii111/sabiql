use super::*;

#[test]
fn sql_modal_keyword_and_number_use_syntax_colors() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content("SELECT 42".to_string());
    state.sql_modal.enter_normal();

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let keyword_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            buffer.cell((x, y)).and_then(|cell| {
                (cell.symbol() == "S" && cell.fg == DEFAULT_THEME.component.syntax.sql_keyword)
                    .then_some(cell)
            })
        });
    let number_cell = (0..TEST_HEIGHT)
        .flat_map(|y| (0..TEST_WIDTH).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            buffer.cell((x, y)).and_then(|cell| {
                (cell.symbol() == "4" && cell.fg == DEFAULT_THEME.component.syntax.sql_number)
                    .then_some(cell)
            })
        });

    assert!(keyword_cell.is_some(), "Expected a blue SQL keyword cell");
    assert!(
        keyword_cell
            .expect("keyword cell should exist")
            .modifier
            .contains(Modifier::BOLD)
    );
    assert!(number_cell.is_some(), "Expected a yellow SQL number cell");
}
#[test]
fn sql_modal_string_comment_and_operator_use_syntax_colors() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content("SELECT 'x'::text -- note".to_string());
    state.sql_modal.enter_editing();

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let has_string = has_cell(&buffer, |cell| {
        cell.symbol() == "'" && cell.fg == DEFAULT_THEME.component.syntax.sql_string
    });
    let has_operator = has_cell(&buffer, |cell| {
        cell.symbol() == ":" && cell.fg == DEFAULT_THEME.component.syntax.sql_operator
    });
    let has_comment = has_cell(&buffer, |cell| {
        cell.symbol() == "-" && cell.fg == DEFAULT_THEME.component.syntax.sql_comment
    });

    assert!(has_string, "Expected a green SQL string cell");
    assert!(has_operator, "Expected a cyan SQL operator cell");
    assert!(has_comment, "Expected a dark gray SQL comment cell");
}

#[test]
fn test_contrast_theme_applies_sql_syntax_colors() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content("SELECT 'x' + 42 -- note".to_string());
    state.sql_modal.enter_editing();

    let buffer = render_and_get_buffer_at_with_theme(
        &mut terminal,
        &mut state,
        Instant::now(),
        &TEST_CONTRAST_THEME,
    );

    let has_keyword = has_cell(&buffer, |cell| {
        cell.symbol() == "S" && cell.fg == TEST_CONTRAST_THEME.component.syntax.sql_keyword
    });
    let has_string = has_cell(&buffer, |cell| {
        cell.symbol() == "'" && cell.fg == TEST_CONTRAST_THEME.component.syntax.sql_string
    });
    let has_comment = has_cell(&buffer, |cell| {
        cell.symbol() == "-" && cell.fg == TEST_CONTRAST_THEME.component.syntax.sql_comment
    });
    let has_number = has_cell(&buffer, |cell| {
        cell.symbol() == "4" && cell.fg == TEST_CONTRAST_THEME.component.syntax.sql_number
    });
    let has_operator = has_cell(&buffer, |cell| {
        cell.symbol() == "+" && cell.fg == TEST_CONTRAST_THEME.component.syntax.sql_operator
    });

    assert!(
        has_keyword,
        "Expected SQL keyword color to resolve from TEST_CONTRAST_THEME"
    );
    assert!(
        has_string,
        "Expected SQL string color to resolve from TEST_CONTRAST_THEME"
    );
    assert!(
        has_comment,
        "Expected SQL comment color to resolve from TEST_CONTRAST_THEME"
    );
    assert!(
        has_number,
        "Expected SQL number color to resolve from TEST_CONTRAST_THEME"
    );
    assert!(
        has_operator,
        "Expected SQL operator color to resolve from TEST_CONTRAST_THEME"
    );
}

#[test]
fn sql_modal_unterminated_string_keeps_string_highlight() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content("SELECT 'unterminated".to_string());
    state.sql_modal.enter_editing();

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let has_string = has_cell(&buffer, |cell| {
        (cell.symbol() == "'" || cell.symbol() == "u")
            && cell.fg == DEFAULT_THEME.component.syntax.sql_string
    });

    assert!(
        has_string,
        "Expected unterminated string input to keep SQL string highlight"
    );
}

#[test]
fn sql_modal_unterminated_block_comment_keeps_comment_highlight() {
    let mut state = create_test_state();
    let mut terminal = create_test_terminal();

    state.modal.set_mode(InputMode::SqlModal);
    state
        .sql_modal
        .editor_mut_for_input()
        .set_content("SELECT /* pending".to_string());
    state.sql_modal.enter_editing();

    let buffer = render_and_get_buffer(&mut terminal, &mut state);

    let has_comment = has_cell(&buffer, |cell| {
        (cell.symbol() == "/" || cell.symbol() == "*")
            && cell.fg == DEFAULT_THEME.component.syntax.sql_comment
    });

    assert!(
        has_comment,
        "Expected unterminated block comment input to keep SQL comment highlight"
    );
}
