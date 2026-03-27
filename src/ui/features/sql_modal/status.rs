use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::model::app_state::AppState;
use crate::app::model::sql_editor::modal::{HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalStatus};
use crate::ui::primitives::atoms::{spinner_char, text_cursor_spans};
use crate::ui::theme::Theme;

pub(super) fn render_status(frame: &mut Frame, area: Rect, state: &AppState) {
    if let SqlModalStatus::ConfirmingHigh {
        decision,
        input,
        target_name,
    } = state.sql_modal.status()
    {
        render_confirming_high_status(frame, area, decision, input, target_name.as_ref());
        return;
    }

    let (badge_text, badge_style, status_text, status_style) = match state.sql_modal.status() {
        SqlModalStatus::Normal => {
            if let Some(ref msg) = state.messages.last_success {
                (
                    "[NORMAL]",
                    Style::default().fg(Theme::TEXT_DIM),
                    format!("\u{2713} {msg}"),
                    Style::default().fg(Theme::STATUS_SUCCESS),
                )
            } else {
                (
                    "[NORMAL]",
                    Style::default().fg(Theme::TEXT_DIM),
                    "Ready".to_string(),
                    Style::default().fg(Theme::TEXT_DIM),
                )
            }
        }
        SqlModalStatus::Editing => (
            "[INSERT]",
            Style::default()
                .fg(Theme::TEXT_ACCENT)
                .add_modifier(Modifier::BOLD),
            "Ready".to_string(),
            Style::default().fg(Theme::TEXT_DIM),
        ),
        SqlModalStatus::Confirming(decision) => {
            let text = format!(
                "\u{26a0} {} RISK  {}",
                decision.risk_level.as_str(),
                decision.label
            );
            let color = Style::default().fg(Theme::risk_color(decision.risk_level));
            ("[CONFIRM]", color, text, color)
        }
        SqlModalStatus::Running => {
            let elapsed = state
                .query
                .start_time()
                .map(|t| t.elapsed())
                .unwrap_or_default();
            let spinner = spinner_char(elapsed.as_millis());
            let elapsed_secs = elapsed.as_secs_f32();
            let status = format!("{spinner} Running {elapsed_secs:.1}s");
            (
                "[RUNNING]",
                Style::default().fg(Theme::TEXT_ACCENT),
                status,
                Style::default().fg(Theme::TEXT_ACCENT),
            )
        }
        SqlModalStatus::Success => {
            let msg = success_status_message(state);
            (
                "[NORMAL]",
                Style::default().fg(Theme::STATUS_SUCCESS),
                msg,
                Style::default()
                    .fg(Theme::STATUS_SUCCESS)
                    .add_modifier(Modifier::BOLD),
            )
        }
        SqlModalStatus::Error => {
            let msg = error_status_message(state);
            (
                "[NORMAL]",
                Style::default().fg(Theme::STATUS_ERROR),
                msg,
                Style::default()
                    .fg(Theme::STATUS_ERROR)
                    .add_modifier(Modifier::BOLD),
            )
        }
        SqlModalStatus::ConfirmingAnalyze { is_dml, .. } => {
            let color = if *is_dml {
                Theme::STATUS_ERROR
            } else {
                Theme::STATUS_WARNING
            };
            (
                "[CONFIRM]",
                Style::default().fg(color).add_modifier(Modifier::BOLD),
                "Confirm ANALYZE".to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )
        }
        SqlModalStatus::ConfirmingAnalyzeHigh { .. } => (
            "[CONFIRM]",
            Style::default()
                .fg(Theme::STATUS_ERROR)
                .add_modifier(Modifier::BOLD),
            "Confirm ANALYZE".to_string(),
            Style::default()
                .fg(Theme::STATUS_ERROR)
                .add_modifier(Modifier::BOLD),
        ),
        SqlModalStatus::ConfirmingHigh { .. } => unreachable!(),
    };

    let badge_display = format!(" {badge_text}");
    let badge_width = badge_display.len() as u16;
    let [badge_area, status_area] =
        Layout::horizontal([Constraint::Length(badge_width + 1), Constraint::Min(1)]).areas(area);

    let badge_line = Line::from(Span::styled(badge_display, badge_style));
    frame.render_widget(Paragraph::new(badge_line), badge_area);

    let status_display = format!("{status_text} ");
    let status_line = Line::from(vec![Span::styled(status_display, status_style)]);
    frame.render_widget(
        Paragraph::new(status_line).alignment(ratatui::layout::Alignment::Right),
        status_area,
    );
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
    format!("{truncated}\u{2026}")
}

fn render_confirming_high_status(
    frame: &mut Frame,
    area: Rect,
    decision: &crate::app::policy::write::write_guardrails::AdhocRiskDecision,
    input: &crate::app::model::shared::text_input::TextInputState,
    target_name: Option<&String>,
) {
    let error_style = Style::default().fg(Theme::STATUS_ERROR);

    if let Some(name) = target_name {
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
        let display_name = truncate_with_ellipsis(name, max_name_display);
        let prompt = format!("Confirm \"{display_name}\": > ");
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
    } else {
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

fn success_status_message(state: &AppState) -> String {
    let Some(snapshot) = state.sql_modal.last_adhoc_success() else {
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
        .sql_modal
        .last_adhoc_error()
        .and_then(|e| e.lines().next())
        .map_or_else(
            || "\u{2717} Error".to_string(),
            |line| format!("\u{2717} {line}"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod truncate_with_ellipsis_tests {
        use super::*;

        #[rstest]
        #[case("users", 16, "users")]
        #[case("user_sessions", 16, "user_sessions")]
        #[case("exactly_16_chars", 16, "exactly_16_chars")]
        #[case("public.user_sessions", 16, "public.user_ses\u{2026}")]
        #[case("my_schema.very_long_table_name", 16, "my_schema.very_\u{2026}")]
        #[case("ab", 1, "\u{2026}")]
        fn truncates_long_names(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
            assert_eq!(truncate_with_ellipsis(input, max), expected);
        }

        #[test]
        fn zero_max_returns_ellipsis() {
            assert_eq!(truncate_with_ellipsis("anything", 0), "\u{2026}");
        }

        #[test]
        fn multibyte_truncates_by_char_count() {
            let result = truncate_with_ellipsis("テーブル名前", 4);

            assert_eq!(result, "テーブ\u{2026}");
        }
    }
}
