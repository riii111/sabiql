use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::text_input::TextInputState;
use crate::app::model::sql_editor::modal::{
    AdhocSuccessSnapshot, HIGH_RISK_INPUT_VISIBLE_WIDTH, SqlModalStatus,
};
use crate::app::policy::write::sql_risk::AcknowledgeReason;
use crate::app::policy::write::write_guardrails::AdhocRiskDecision;
use crate::domain::{CommandTag, DatabaseDiagnostic, DiagnosticLevel};
use crate::primitives::atoms::{spinner_char, text_cursor_spans};
use crate::primitives::utils::text_utils::{truncate_to_width_with, wrapped_line_count};
use crate::theme::ThemePalette;

pub(super) fn status_height(state: &AppState, width: u16) -> u16 {
    let SqlModalStatus::Success(snapshot) = state.sql_modal.status() else {
        return 1;
    };
    if snapshot.mysql_diagnostics.is_empty() {
        return 1;
    }

    let status_width = width.saturating_sub(" [NORMAL]".len() as u16 + 1);
    let base_lines = wrapped_line_count(&success_status_message(snapshot), status_width);
    let diagnostic_lines = snapshot
        .mysql_diagnostics
        .iter()
        .map(|diagnostic| wrapped_line_count(&diagnostic_status_line(diagnostic), status_width))
        .sum::<u16>();
    base_lines.saturating_add(diagnostic_lines).max(1)
}

pub(super) fn render_status(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
    if let SqlModalStatus::ConfirmingHigh {
        decision,
        input,
        target_name,
    } = state.sql_modal.status()
    {
        render_confirming_high_status(frame, area, decision, input, target_name, theme);
        return;
    }

    if let SqlModalStatus::ConfirmingRisk { reason, label } = state.sql_modal.status() {
        render_confirming_risk_status(frame, area, reason, label, theme);
        return;
    }

    if let SqlModalStatus::Success(snapshot) = state.sql_modal.status()
        && !snapshot.mysql_diagnostics.is_empty()
    {
        render_success_status_with_diagnostics(frame, area, snapshot, theme);
        return;
    }

    let (badge_text, badge_style, status_text, status_style) = match state.sql_modal.status() {
        SqlModalStatus::Normal => {
            if let Some(msg) = state.messages.last_success() {
                (
                    "[NORMAL]",
                    Style::default().fg(theme.semantic.text.dim),
                    format!("\u{2713} {msg}"),
                    Style::default().fg(theme.semantic.status.success),
                )
            } else {
                (
                    "[NORMAL]",
                    Style::default().fg(theme.semantic.text.dim),
                    "Ready".to_string(),
                    Style::default().fg(theme.semantic.text.dim),
                )
            }
        }
        SqlModalStatus::Editing => (
            "[INSERT]",
            Style::default()
                .fg(theme.semantic.text.accent)
                .add_modifier(Modifier::BOLD),
            "Ready".to_string(),
            Style::default().fg(theme.semantic.text.dim),
        ),
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
                Style::default().fg(theme.semantic.text.accent),
                status,
                Style::default().fg(theme.semantic.text.accent),
            )
        }
        SqlModalStatus::Success(snapshot) => {
            let msg = success_status_message(snapshot);
            (
                "[NORMAL]",
                Style::default().fg(theme.semantic.status.success),
                msg,
                Style::default()
                    .fg(theme.semantic.status.success)
                    .add_modifier(Modifier::BOLD),
            )
        }
        SqlModalStatus::Error(error) => {
            let msg = error_status_message(error);
            (
                "[NORMAL]",
                Style::default().fg(theme.semantic.status.error),
                msg,
                Style::default()
                    .fg(theme.semantic.status.error)
                    .add_modifier(Modifier::BOLD),
            )
        }
        SqlModalStatus::ConfirmingAnalyzeHigh { .. } => (
            "[CONFIRM]",
            Style::default()
                .fg(theme.semantic.status.error)
                .add_modifier(Modifier::BOLD),
            "Confirm ANALYZE".to_string(),
            Style::default()
                .fg(theme.semantic.status.error)
                .add_modifier(Modifier::BOLD),
        ),
        SqlModalStatus::ConfirmingAnalyzeRisk { .. } => (
            "[CONFIRM]",
            Style::default()
                .fg(theme.semantic.status.error)
                .add_modifier(Modifier::BOLD),
            "Execute ANALYZE".to_string(),
            Style::default()
                .fg(theme.semantic.status.error)
                .add_modifier(Modifier::BOLD),
        ),
        SqlModalStatus::ConfirmingHigh { .. } | SqlModalStatus::ConfirmingRisk { .. } => {
            unreachable!()
        }
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

fn render_confirming_high_status(
    frame: &mut Frame,
    area: Rect,
    decision: &AdhocRiskDecision,
    input: &TextInputState,
    name: &str,
    theme: &ThemePalette,
) {
    let error_style = Style::default().fg(theme.semantic.status.error);

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
            Style::default().fg(theme.semantic.text.muted),
        ));
    }
    let line1 = Line::from(line1_spans);

    let prompt_fixed_len = "Confirm \"\": > ".len();
    let max_name_display =
        (area.width as usize).saturating_sub(prompt_fixed_len + HIGH_RISK_INPUT_VISIBLE_WIDTH + 2);
    let display_name = truncate_to_width_with(name, max_name_display, "\u{2026}");
    let prompt = format!("Confirm \"{display_name}\": > ");
    let visible_width = HIGH_RISK_INPUT_VISIBLE_WIDTH;
    let cursor_spans = text_cursor_spans(
        input.content(),
        input.cursor(),
        input.viewport_offset(),
        visible_width,
        theme,
    );
    let mut line2_spans = vec![Span::styled(
        prompt,
        Style::default().fg(theme.semantic.text.secondary),
    )];
    line2_spans.extend(cursor_spans);
    if is_match {
        line2_spans.push(Span::styled(
            " \u{2713}",
            Style::default().fg(theme.semantic.status.success),
        ));
    }
    let line2 = Line::from(line2_spans);

    let paragraph = Paragraph::new(vec![line1, line2]);
    frame.render_widget(paragraph, area);
}

fn render_confirming_risk_status(
    frame: &mut Frame,
    area: Rect,
    reason: &AcknowledgeReason,
    label: &str,
    theme: &ThemePalette,
) {
    let (badge_text, badge_style, explanation) = match reason {
        AcknowledgeReason::UnknownRisk => (
            format!("\u{26a0} UNKNOWN RISK  {label}"),
            Style::default().fg(theme.semantic.status.warning),
            "sabiql can't assess this statement's risk",
        ),
        AcknowledgeReason::TargetNameUnavailable => (
            format!("\u{26a0} HIGH RISK  {label}"),
            Style::default().fg(theme.semantic.status.error),
            "Can't identify target name \u{2014} review before executing",
        ),
        AcknowledgeReason::NonAtomicTransaction => (
            "\u{26a0} NON-ATOMIC  SQLite transaction".to_string(),
            Style::default().fg(theme.semantic.status.warning),
            "SQLite must run this script without an automatic transaction",
        ),
        AcknowledgeReason::AnalyzeExecution => (
            format!("\u{26a0} LOW RISK  {label}"),
            Style::default().fg(theme.semantic.status.warning),
            "EXPLAIN ANALYZE will execute this read-only statement",
        ),
    };

    let line1 = Line::from(Span::styled(badge_text, badge_style));
    let line2 = Line::from(Span::styled(
        explanation,
        Style::default().fg(theme.semantic.text.muted),
    ));
    frame.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn success_status_message(snapshot: &AdhocSuccessSnapshot) -> String {
    let time_secs = snapshot.execution_time_ms as f64 / 1000.0;

    if let Some(tag) = snapshot.command_tag.as_ref() {
        format!("\u{2713} {} ({:.2}s)", command_tag_message(tag), time_secs)
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

fn command_tag_message(tag: &CommandTag) -> String {
    match tag {
        CommandTag::Select(n) => row_count_label(*n, "selected"),
        CommandTag::Insert(n) => row_count_label(*n, "inserted"),
        CommandTag::Affected(n) => row_count_label(*n, "affected"),
        CommandTag::Update(n) => row_count_label(*n, "updated"),
        CommandTag::Delete(n) => row_count_label(*n, "deleted"),
        CommandTag::Create(obj) => format!("{} created", obj.to_lowercase()),
        CommandTag::Drop(obj) => format!("{} dropped", obj.to_lowercase()),
        CommandTag::Alter(obj) => format!("{} altered", obj.to_lowercase()),
        CommandTag::Truncate => "table truncated".to_string(),
        CommandTag::Begin => "transaction started".to_string(),
        CommandTag::Commit => "committed".to_string(),
        CommandTag::Rollback => "rolled back".to_string(),
        CommandTag::Other(tag) => tag.to_lowercase(),
    }
}

fn row_count_label(n: u64, verb: &str) -> String {
    if n == 1 {
        format!("1 row {verb}")
    } else {
        format!("{n} rows {verb}")
    }
}

fn render_success_status_with_diagnostics(
    frame: &mut Frame,
    area: Rect,
    snapshot: &AdhocSuccessSnapshot,
    theme: &ThemePalette,
) {
    let badge_display = " [NORMAL]";
    let badge_width = badge_display.len() as u16 + 1;
    let [badge_area, status_area] =
        Layout::horizontal([Constraint::Length(badge_width), Constraint::Min(1)]).areas(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            badge_display,
            Style::default().fg(theme.semantic.status.success),
        ))),
        badge_area,
    );

    let mut lines = vec![Line::from(Span::styled(
        format!("{} ", success_status_message(snapshot)),
        Style::default()
            .fg(theme.semantic.status.success)
            .add_modifier(Modifier::BOLD),
    ))];
    lines.extend(snapshot.mysql_diagnostics.iter().map(|diagnostic| {
        Line::from(Span::styled(
            diagnostic_status_line(diagnostic),
            Style::default().fg(theme.semantic.status.warning),
        ))
    }));
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        status_area,
    );
}

fn diagnostic_status_line(diagnostic: &DatabaseDiagnostic) -> String {
    let level = match diagnostic.level {
        DiagnosticLevel::Warning => "Warning",
        DiagnosticLevel::Note => "Note",
    };
    format!(
        "⚠ {level} (Code {}): {}",
        diagnostic.code, diagnostic.message
    )
}

fn error_status_message(error: &str) -> String {
    error.lines().next().map_or_else(
        || "\u{2717} Error".to_string(),
        |line| format!("\u{2717} {line}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_tag_message_singular_row() {
        assert_eq!(
            command_tag_message(&CommandTag::Insert(1)),
            "1 row inserted"
        );
        assert_eq!(
            command_tag_message(&CommandTag::Affected(1)),
            "1 row affected"
        );
        assert_eq!(command_tag_message(&CommandTag::Delete(1)), "1 row deleted");
    }

    #[test]
    fn command_tag_message_plural_rows() {
        assert_eq!(
            command_tag_message(&CommandTag::Select(5)),
            "5 rows selected"
        );
        assert_eq!(
            command_tag_message(&CommandTag::Update(10)),
            "10 rows updated"
        );
    }

    #[test]
    fn command_tag_message_zero_rows() {
        assert_eq!(
            command_tag_message(&CommandTag::Delete(0)),
            "0 rows deleted"
        );
        assert_eq!(
            command_tag_message(&CommandTag::Affected(0)),
            "0 rows affected"
        );
    }

    #[test]
    fn command_tag_message_ddl() {
        assert_eq!(
            command_tag_message(&CommandTag::Create("TABLE".to_string())),
            "table created"
        );
        assert_eq!(
            command_tag_message(&CommandTag::Drop("INDEX".to_string())),
            "index dropped"
        );
        assert_eq!(
            command_tag_message(&CommandTag::Alter("TABLE".to_string())),
            "table altered"
        );
    }

    #[test]
    fn command_tag_message_tcl() {
        assert_eq!(
            command_tag_message(&CommandTag::Truncate),
            "table truncated"
        );
        assert_eq!(
            command_tag_message(&CommandTag::Begin),
            "transaction started"
        );
        assert_eq!(command_tag_message(&CommandTag::Commit), "committed");
        assert_eq!(command_tag_message(&CommandTag::Rollback), "rolled back");
    }

    #[test]
    fn command_tag_message_other() {
        assert_eq!(
            command_tag_message(&CommandTag::Other("VACUUM".to_string())),
            "vacuum"
        );
    }

    #[test]
    fn diagnostics_height_only_applies_to_success_status() {
        let mut state = AppState::new("test_project".to_string());
        state.sql_modal.finish_adhoc_success(AdhocSuccessSnapshot {
            command_tag: None,
            row_count: 1,
            execution_time_ms: 15,
            mysql_diagnostics: vec![DatabaseDiagnostic {
                level: DiagnosticLevel::Warning,
                code: 1265,
                message: "truncated".to_string(),
            }],
        });

        assert!(status_height(&state, 80) > 1);
        state.sql_modal.enter_normal();
        assert_eq!(status_height(&state, 80), 1);

        state.sql_modal.enter_editing();
        assert_eq!(status_height(&state, 80), 1);

        state.sql_modal.begin_adhoc_running();
        assert_eq!(status_height(&state, 80), 1);

        state.sql_modal.finish_adhoc_error("error".to_string());
        assert_eq!(status_height(&state, 80), 1);
    }
}
