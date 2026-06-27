use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::sqlite::diagnostics::display_lines;
use crate::domain::SqliteDiagnosticsSnapshot;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

pub struct SqliteDiagnosticsOverlay;

impl SqliteDiagnosticsOverlay {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            Constraint::Percentage(60),
            " SQLite Diagnostics ",
            FooterHintBar::new([("Esc", "Close"), ("↑↓", "Scroll")]),
            theme,
        );

        if state.sqlite_diagnostics.is_loading() {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::raw(""),
                    Line::from(Span::styled(
                        "Loading diagnostics...",
                        Style::default().fg(theme.semantic.status.warning),
                    )),
                ]),
                inner,
            );
            return;
        }

        let Some(snapshot) = state.sqlite_diagnostics.snapshot() else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Diagnostics unavailable.",
                    Style::default().fg(theme.semantic.status.error),
                ))),
                inner,
            );
            return;
        };

        let lines = render_lines(snapshot, theme);
        let scroll = state.sqlite_diagnostics.scroll_offset();
        let visible = lines
            .into_iter()
            .skip(scroll)
            .take(inner.height as usize)
            .collect::<Vec<_>>();

        frame.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), inner);
    }
}

fn render_lines(snapshot: &SqliteDiagnosticsSnapshot, theme: &ThemePalette) -> Vec<Line<'static>> {
    let mut lines = vec![Line::raw("")];
    for (label, value) in display_lines(snapshot) {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{label}: "),
                Style::default()
                    .fg(theme.semantic.text.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(value, field_style(snapshot, &label, theme)),
        ]));
        lines.push(Line::raw(""));
    }
    lines
}

fn field_style(snapshot: &SqliteDiagnosticsSnapshot, label: &str, theme: &ThemePalette) -> Style {
    let field = match label {
        "Database file" => &snapshot.db_file,
        "SQLite version" => &snapshot.sqlite_version,
        "Foreign keys" => &snapshot.foreign_keys,
        "Journal mode" => &snapshot.journal_mode,
        "Query only" => &snapshot.query_only,
        "Busy timeout (ms)" => &snapshot.busy_timeout,
        "Attached databases" => &snapshot.database_list,
        "Quick check" => &snapshot.quick_check,
        _ => return Style::default().fg(theme.semantic.text.secondary),
    };

    if field.is_ok() {
        if label == "Quick check"
            && snapshot
                .quick_check_result()
                .is_some_and(|result| !result.is_ok)
        {
            Style::default().fg(theme.semantic.status.error)
        } else if label == "Quick check"
            && snapshot
                .quick_check_result()
                .is_some_and(|result| result.is_ok)
        {
            Style::default().fg(theme.semantic.status.success)
        } else {
            Style::default().fg(theme.semantic.text.secondary)
        }
    } else {
        Style::default().fg(theme.semantic.status.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::theme_id::ThemeId;
    use crate::domain::DiagnosticField;
    use crate::theme::palette_for;

    #[test]
    fn quick_check_failure_uses_error_style() {
        let snapshot = SqliteDiagnosticsSnapshot {
            quick_check: DiagnosticField::ok("row 1 missing from index idx_users"),
            ..Default::default()
        };
        let theme = palette_for(ThemeId::Default);

        let style = field_style(&snapshot, "Quick check", theme);

        assert_eq!(style.fg, Some(theme.semantic.status.error));
    }
}
