use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::app::sql_modal_context::{
    CompletionKind, HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalStatus,
};
use crate::app::state::AppState;
use crate::ui::theme::Theme;

use super::atoms::{spinner_char, text_cursor_spans};
use super::molecules::{render_modal, render_modal_with_border_color};

pub struct SqlModal;

impl SqlModal {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let (area, inner) = match &state.sql_modal.status {
            SqlModalStatus::Confirming(decision) => {
                let title = format!(
                    " SQL \u{2500}\u{2500} \u{26a0} {} ",
                    decision.risk_level.as_str()
                );
                render_modal_with_border_color(
                    frame,
                    Constraint::Percentage(80),
                    Constraint::Percentage(60),
                    &title,
                    " Enter: Execute \u{2502} Esc: Back ",
                    Theme::risk_color(decision.risk_level),
                )
            }
            SqlModalStatus::ConfirmingHigh {
                decision,
                input,
                target_name,
            } => {
                let title = format!(
                    " SQL \u{2500}\u{2500} \u{26a0} {} ",
                    decision.risk_level.as_str()
                );
                let is_match = target_name
                    .as_ref()
                    .is_some_and(|name| input.content() == name);
                let footer = if is_match {
                    " Enter: Execute \u{2502} Esc: Back "
                } else {
                    " Esc: Back "
                };
                render_modal_with_border_color(
                    frame,
                    Constraint::Percentage(80),
                    Constraint::Percentage(60),
                    &title,
                    footer,
                    Theme::STATUS_ERROR,
                )
            }
            _ => render_modal(
                frame,
                Constraint::Percentage(80),
                Constraint::Percentage(60),
                " SQL Editor ",
                " Alt+Enter: Run \u{2502} Ctrl+L: Clear \u{2502} Esc: Close ",
            ),
        };

        let status_height = if matches!(
            state.sql_modal.status,
            SqlModalStatus::ConfirmingHigh { .. }
        ) {
            3 // warning line + input prompt line + bottom margin
        } else {
            1
        };

        let [editor_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(status_height)]).areas(inner);

        Self::render_editor(frame, editor_area, state);
        Self::render_status(frame, status_area, state);

        let is_confirming = matches!(
            state.sql_modal.status,
            SqlModalStatus::Confirming(_) | SqlModalStatus::ConfirmingHigh { .. }
        );
        if !is_confirming
            && state.sql_modal.completion.visible
            && !state.sql_modal.completion.candidates.is_empty()
        {
            Self::render_completion_popup(frame, area, editor_area, state);
        }
    }

    fn render_editor(frame: &mut Frame, area: Rect, state: &AppState) {
        let content = &state.sql_modal.content;

        // Cursor and highlight are omitted to reinforce that the SQL is not editable here.
        if matches!(
            state.sql_modal.status,
            SqlModalStatus::Confirming(_) | SqlModalStatus::ConfirmingHigh { .. }
        ) {
            let lines: Vec<Line> = content
                .lines()
                .map(|line| {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(Theme::TEXT_MUTED),
                    ))
                })
                .collect();
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
            return;
        }

        let cursor_pos = state.sql_modal.cursor;
        let (cursor_row, cursor_col) = Self::cursor_to_position(content, cursor_pos);
        let current_line_style = Style::default().bg(Theme::EDITOR_CURRENT_LINE_BG);

        let mut lines: Vec<Line> = if content.is_empty() {
            vec![
                Line::from(vec![
                    Span::styled("\u{2588}", Style::default().fg(Theme::CURSOR_FG)),
                    Span::styled(
                        " Enter SQL query...",
                        Style::default().fg(Theme::PLACEHOLDER_TEXT),
                    ),
                ])
                .style(current_line_style),
            ]
        } else {
            content
                .lines()
                .enumerate()
                .map(|(row, line)| {
                    if row == cursor_row {
                        Self::line_with_cursor(line, cursor_col).style(current_line_style)
                    } else {
                        Line::from(line.to_string())
                    }
                })
                .collect()
        };

        if content.ends_with('\n') && cursor_row == content.lines().count() {
            lines.push(
                Line::from(vec![Span::styled(
                    "\u{2588}",
                    Style::default().fg(Theme::CURSOR_FG),
                )])
                .style(current_line_style),
            );
        }

        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .style(Style::default()),
            area,
        );
    }

    fn line_with_cursor(line: &str, cursor_col: usize) -> Line<'static> {
        Line::from(text_cursor_spans(line, cursor_col, 0, usize::MAX))
    }

    fn render_status(frame: &mut Frame, area: Rect, state: &AppState) {
        if let SqlModalStatus::ConfirmingHigh {
            decision,
            input,
            target_name,
        } = &state.sql_modal.status
        {
            Self::render_confirming_high_status(frame, area, decision, input, target_name);
            return;
        }

        let (status_text, status_style) = match state.sql_modal.status {
            SqlModalStatus::Editing => {
                ("Ready".to_string(), Style::default().fg(Theme::TEXT_MUTED))
            }
            SqlModalStatus::Confirming(decision) => {
                let text = format!(
                    "\u{26a0} {} RISK  {}",
                    decision.risk_level.as_str(),
                    decision.label
                );
                (
                    text,
                    Style::default().fg(Theme::risk_color(decision.risk_level)),
                )
            }
            SqlModalStatus::Running => {
                let elapsed = state
                    .query
                    .start_time
                    .map(|t| t.elapsed())
                    .unwrap_or_default();
                let spinner = spinner_char(elapsed.as_millis());
                let elapsed_secs = elapsed.as_secs_f32();
                let status = format!("{} Running {:.1}s", spinner, elapsed_secs);
                (status, Style::default().fg(Theme::TEXT_ACCENT))
            }
            SqlModalStatus::Success => {
                let msg = Self::success_status_message(state);
                (
                    msg,
                    Style::default()
                        .fg(Theme::STATUS_SUCCESS)
                        .add_modifier(Modifier::BOLD),
                )
            }
            SqlModalStatus::Error => {
                let msg = Self::error_status_message(state);
                (
                    msg,
                    Style::default()
                        .fg(Theme::STATUS_ERROR)
                        .add_modifier(Modifier::BOLD),
                )
            }
            SqlModalStatus::ConfirmingHigh { .. } => unreachable!(),
        };

        let line = Line::from(vec![Span::styled(status_text, status_style)]);
        frame.render_widget(Paragraph::new(line).style(Style::default()), area);
    }

    fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
        if max_chars == 0 {
            return "\u{2026}".to_string();
        }
        let char_count = s.chars().count();
        if char_count <= max_chars {
            return s.to_string();
        }
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}\u{2026}", truncated)
    }

    fn render_confirming_high_status(
        frame: &mut Frame,
        area: Rect,
        decision: &crate::app::write_guardrails::AdhocRiskDecision,
        input: &crate::app::text_input::TextInputState,
        target_name: &Option<String>,
    ) {
        let error_style = Style::default().fg(Theme::STATUS_ERROR);

        match target_name {
            Some(name) => {
                let is_match = input.content() == name;
                let warning_text = format!("\u{26a0} HIGH RISK  {}", decision.label);
                let blocked_label = "Enter blocked";
                let mut line1_spans = vec![Span::styled(warning_text.clone(), error_style)];
                if !is_match {
                    let used = (warning_text.len() + blocked_label.len()) as u16;
                    let padding = area.width.saturating_sub(used).max(2);
                    line1_spans.push(Span::raw(" ".repeat(padding as usize)));
                    line1_spans.push(Span::styled(
                        blocked_label,
                        Style::default().fg(Theme::TEXT_MUTED),
                    ));
                }
                let line1 = Line::from(line1_spans);

                let prompt_fixed_len = "Confirm \"\": > ".len();
                let max_name_display = (area.width as usize)
                    .saturating_sub(prompt_fixed_len + HIGH_RISK_INPUT_VISIBLE_WIDTH + 2);
                let display_name = Self::truncate_with_ellipsis(name, max_name_display);
                let prompt = format!("Confirm \"{}\": > ", display_name);
                let visible_width = HIGH_RISK_INPUT_VISIBLE_WIDTH;
                let cursor_spans = text_cursor_spans(
                    input.content(),
                    input.cursor(),
                    input.viewport_offset(),
                    visible_width,
                );
                let mut line2_spans = vec![Span::styled(
                    prompt,
                    Style::default().fg(Theme::TEXT_SECONDARY),
                )];
                line2_spans.extend(cursor_spans);
                if is_match {
                    line2_spans.push(Span::styled(
                        " \u{2713}",
                        Style::default().fg(Theme::STATUS_SUCCESS),
                    ));
                }
                let line2 = Line::from(line2_spans);

                let paragraph = Paragraph::new(vec![line1, line2]);
                frame.render_widget(paragraph, area);
            }
            None => {
                let line1 = Line::from(Span::styled(
                    format!("\u{26a0} HIGH RISK  {}", decision.label),
                    error_style,
                ));
                let line2 = Line::from(Span::styled(
                    "Cannot execute: unable to identify target table.  Esc: Back",
                    Style::default().fg(Theme::TEXT_MUTED),
                ));
                let paragraph = Paragraph::new(vec![line1, line2]);
                frame.render_widget(paragraph, area);
            }
        }
    }

    fn success_status_message(state: &AppState) -> String {
        let Some(snapshot) = state.sql_modal.last_adhoc_success.as_ref() else {
            return "\u{2713} OK".to_string();
        };
        let time_secs = snapshot.execution_time_ms as f64 / 1000.0;

        if let Some(tag) = snapshot.command_tag.as_ref() {
            format!("\u{2713} {} ({:.2}s)", tag.display_message(), time_secs)
        } else {
            let rows_label = if snapshot.row_count == 1 {
                "row"
            } else {
                "rows"
            };
            format!(
                "\u{2713} {} {} ({:.2}s)",
                snapshot.row_count, rows_label, time_secs
            )
        }
    }

    fn error_status_message(state: &AppState) -> String {
        state
            .query
            .current_result
            .as_ref()
            .and_then(|r| r.error.as_ref())
            .and_then(|e| e.lines().next())
            .map(|line| format!("\u{2717} {}", line))
            .unwrap_or_else(|| "\u{2717} Error".to_string())
    }

    fn render_completion_popup(
        frame: &mut Frame,
        modal_area: Rect,
        editor_area: Rect,
        state: &AppState,
    ) {
        let (cursor_row, cursor_col) =
            Self::cursor_to_position(&state.sql_modal.content, state.sql_modal.cursor);

        let max_items = 8;
        let visible_count = state.sql_modal.completion.candidates.len().min(max_items);
        let popup_height = (visible_count as u16) + 2;
        let popup_width = 45u16.min(modal_area.width);

        let popup_x = if modal_area.width < popup_width {
            modal_area.x
        } else {
            (editor_area.x + cursor_col as u16).min(modal_area.right().saturating_sub(popup_width))
        };
        let cursor_screen_y = editor_area.y + cursor_row as u16;

        let popup_y = if cursor_screen_y + 1 + popup_height > modal_area.bottom() {
            cursor_screen_y.saturating_sub(popup_height)
        } else {
            cursor_screen_y + 1
        };

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        frame.render_widget(Clear, popup_area);

        let selected = state.sql_modal.completion.selected_index;
        let total = state.sql_modal.completion.candidates.len();
        let scroll_offset = if total <= max_items {
            0
        } else {
            let half = max_items / 2;
            if selected < half {
                0
            } else if selected >= total - half {
                total - max_items
            } else {
                selected - half
            }
        };

        let max_text_width = state
            .sql_modal
            .completion
            .candidates
            .iter()
            .skip(scroll_offset)
            .take(max_items)
            .map(|c| c.text.len())
            .max()
            .unwrap_or(0);

        let items: Vec<ListItem> = state
            .sql_modal
            .completion
            .candidates
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(max_items)
            .map(|(i, candidate)| {
                let is_selected = i == selected;

                let kind_label = match candidate.kind {
                    CompletionKind::Keyword => "keyword",
                    CompletionKind::Table => "table",
                    CompletionKind::Column => "column",
                };

                let padding = max_text_width.saturating_sub(candidate.text.len()) + 2;
                let text = format!(
                    " {}{:padding$}{}",
                    candidate.text,
                    "",
                    kind_label,
                    padding = padding
                );

                let style = if is_selected {
                    Style::default()
                        .bg(Theme::COMPLETION_SELECTED_BG)
                        .fg(Theme::TEXT_PRIMARY)
                } else {
                    Style::default().fg(Theme::TEXT_SECONDARY)
                };

                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Theme::MODAL_BORDER))
                .style(Style::default()),
        );

        frame.render_widget(list, popup_area);
    }

    /// NOTE: O(n) on every render — acceptable for typical SQL lengths.
    fn cursor_to_position(content: &str, cursor_pos: usize) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;

        for (current_pos, ch) in content.chars().enumerate() {
            if current_pos >= cursor_pos {
                break;
            }
            if ch == '\n' {
                row += 1;
                col = 0;
            } else {
                col += 1;
            }
        }

        (row, col)
    }
}

#[cfg(test)]
impl SqlModal {
    pub fn position_to_cursor(content: &str, row: usize, col: usize) -> usize {
        let mut current_row = 0;
        let mut current_col = 0;
        let mut cursor_pos = 0;

        for ch in content.chars() {
            if current_row == row && current_col == col {
                return cursor_pos;
            }
            if ch == '\n' {
                if current_row == row {
                    return cursor_pos;
                }
                current_row += 1;
                current_col = 0;
            } else {
                current_col += 1;
            }
            cursor_pos += 1;
        }

        cursor_pos
    }

    pub fn line_lengths(content: &str) -> Vec<usize> {
        content.lines().map(|l| l.chars().count()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn empty_string_returns_zero_position() {
        let result = SqlModal::cursor_to_position("", 0);

        assert_eq!(result, (0, 0));
    }

    #[test]
    fn single_line_ascii_returns_correct_position() {
        let content = "SELECT * FROM users";

        let result = SqlModal::cursor_to_position(content, 7);

        assert_eq!(result, (0, 7));
    }

    #[test]
    fn multiple_lines_returns_correct_row_and_col() {
        let content = "SELECT *\nFROM users\nWHERE id = 1";

        let result = SqlModal::cursor_to_position(content, 17);

        assert_eq!(result, (1, 8));
    }

    #[test]
    fn multibyte_characters_count_correctly() {
        let content = "こんにちは世界";

        let result = SqlModal::cursor_to_position(content, 3);

        assert_eq!(result, (0, 3));
    }

    #[rstest]
    #[case("SELECT 日本語", 7, (0, 7))]
    #[case("SELECT 日本語", 8, (0, 8))]
    #[case("SELECT 日本語", 9, (0, 9))]
    #[case("こんにちは\n世界", 5, (0, 5))]
    #[case("こんにちは\n世界", 6, (1, 0))]
    #[case("こんにちは\n世界", 7, (1, 1))]
    fn multibyte_cursor_positions_are_accurate(
        #[case] input: &str,
        #[case] cursor: usize,
        #[case] expected: (usize, usize),
    ) {
        let result = SqlModal::cursor_to_position(input, cursor);

        assert_eq!(result, expected);
    }

    #[test]
    fn position_to_cursor_converts_back_correctly() {
        let content = "SELECT *\nFROM users";

        let cursor = SqlModal::position_to_cursor(content, 1, 5);

        assert_eq!(cursor, 14);
    }

    #[test]
    fn position_to_cursor_with_multibyte_returns_correct_index() {
        let content = "こんにちは\n世界";

        let cursor = SqlModal::position_to_cursor(content, 1, 2);

        assert_eq!(cursor, 8);
    }

    #[test]
    fn cursor_at_end_of_line_returns_line_length() {
        let content = "SELECT *\nFROM users";

        let cursor = SqlModal::position_to_cursor(content, 0, 100);

        assert_eq!(cursor, 8);
    }

    #[test]
    fn line_lengths_counts_chars_not_bytes() {
        let content = "abc\n日本語\nxyz";

        let lengths = SqlModal::line_lengths(content);

        assert_eq!(lengths, vec![3, 3, 3]);
    }

    #[rstest]
    #[case("", vec![])]
    #[case("single", vec![6])]
    #[case("one\ntwo", vec![3, 3])]
    #[case("あ\nい\nう", vec![1, 1, 1])]
    fn line_lengths_handles_various_inputs(#[case] input: &str, #[case] expected: Vec<usize>) {
        let result = SqlModal::line_lengths(input);

        assert_eq!(result, expected);
    }

    mod truncate_with_ellipsis {
        use super::*;

        #[rstest]
        #[case("users", 16, "users")]
        #[case("user_sessions", 16, "user_sessions")]
        #[case("exactly_16_chars", 16, "exactly_16_chars")]
        #[case("public.user_sessions", 16, "public.user_ses\u{2026}")]
        #[case("my_schema.very_long_table_name", 16, "my_schema.very_\u{2026}")]
        #[case("ab", 1, "\u{2026}")]
        fn truncates_long_names(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
            assert_eq!(SqlModal::truncate_with_ellipsis(input, max), expected);
        }

        #[test]
        fn zero_max_returns_ellipsis() {
            assert_eq!(SqlModal::truncate_with_ellipsis("anything", 0), "\u{2026}");
        }

        #[test]
        fn multibyte_truncates_by_char_count() {
            let result = SqlModal::truncate_with_ellipsis("テーブル名前", 4);

            assert_eq!(result, "テーブ\u{2026}");
        }
    }
}
