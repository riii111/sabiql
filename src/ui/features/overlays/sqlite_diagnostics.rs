use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::sqlite::diagnostics::{DiagnosticFieldKind, display_rows};
use crate::domain::SqliteDiagnosticsSnapshot;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

pub struct SqliteDiagnosticsRenderMetrics {
    pub content_line_count: usize,
    pub viewport_height: usize,
}

pub struct SqliteDiagnosticsOverlay;

impl SqliteDiagnosticsOverlay {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> SqliteDiagnosticsRenderMetrics {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            Constraint::Percentage(60),
            " SQLite Diagnostics ",
            FooterHintBar::new([("Esc", "Close"), ("↑↓", "Scroll")]),
            theme,
        );
        let viewport_height = inner.height as usize;

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
            return SqliteDiagnosticsRenderMetrics {
                content_line_count: 2,
                viewport_height,
            };
        }

        let Some(snapshot) = state.sqlite_diagnostics.snapshot() else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Diagnostics unavailable.",
                    Style::default().fg(theme.semantic.status.error),
                ))),
                inner,
            );
            return SqliteDiagnosticsRenderMetrics {
                content_line_count: 1,
                viewport_height,
            };
        };

        let lines = build_render_lines(snapshot, theme);
        let content_line_count = lines.len();
        let scroll = state.sqlite_diagnostics.scroll_offset();
        let visible = lines
            .into_iter()
            .skip(scroll)
            .take(viewport_height)
            .collect::<Vec<_>>();

        frame.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), inner);

        SqliteDiagnosticsRenderMetrics {
            content_line_count,
            viewport_height,
        }
    }
}

pub fn build_render_lines(
    snapshot: &SqliteDiagnosticsSnapshot,
    theme: &ThemePalette,
) -> Vec<Line<'static>> {
    let label_style = Style::default()
        .fg(theme.semantic.text.primary)
        .add_modifier(Modifier::BOLD);

    let mut lines = vec![Line::raw("")];
    for row in display_rows(snapshot) {
        append_field_lines(
            &mut lines,
            row.kind.label(),
            &row.value,
            label_style,
            field_style(row.kind, snapshot, theme),
        );
    }
    lines
}

fn append_field_lines(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    value: &str,
    label_style: Style,
    value_style: Style,
) {
    let mut value_lines = value.lines();
    if let Some(first) = value_lines.next() {
        lines.push(Line::from(vec![
            Span::styled(format!("{label}: "), label_style),
            Span::styled(first.to_string(), value_style),
        ]));
        for continuation in value_lines {
            lines.push(Line::from(Span::styled(
                format!("  {continuation}"),
                value_style,
            )));
        }
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!("{label}: "), label_style),
            Span::styled(String::new(), value_style),
        ]));
    }
    lines.push(Line::raw(""));
}

fn field_style(
    kind: DiagnosticFieldKind,
    snapshot: &SqliteDiagnosticsSnapshot,
    theme: &ThemePalette,
) -> Style {
    let field = match kind {
        DiagnosticFieldKind::DbFile => &snapshot.db_file,
        DiagnosticFieldKind::SqliteVersion => &snapshot.sqlite_version,
        DiagnosticFieldKind::ForeignKeys => &snapshot.foreign_keys,
        DiagnosticFieldKind::JournalMode => &snapshot.journal_mode,
        DiagnosticFieldKind::QueryOnly => &snapshot.query_only,
        DiagnosticFieldKind::BusyTimeout => &snapshot.busy_timeout,
        DiagnosticFieldKind::DatabaseList => &snapshot.database_list,
        DiagnosticFieldKind::QuickCheck => &snapshot.quick_check,
    };

    if field.is_ok() {
        if kind == DiagnosticFieldKind::QuickCheck
            && snapshot
                .quick_check_result()
                .is_some_and(|result| !result.is_ok)
        {
            Style::default().fg(theme.semantic.status.error)
        } else if kind == DiagnosticFieldKind::QuickCheck
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
    use crate::domain::{DiagnosticField, SqliteDiagnosticsSnapshot};
    use crate::theme::palette_for;

    #[test]
    fn quick_check_failure_uses_error_style() {
        let snapshot = SqliteDiagnosticsSnapshot {
            quick_check: DiagnosticField::ok("row 1 missing from index idx_users"),
            ..Default::default()
        };
        let theme = palette_for(ThemeId::Default);

        let style = field_style(DiagnosticFieldKind::QuickCheck, &snapshot, theme);

        assert_eq!(style.fg, Some(theme.semantic.status.error));
    }

    #[test]
    fn multiline_values_produce_separate_render_lines() {
        let snapshot = SqliteDiagnosticsSnapshot {
            database_list: DiagnosticField::ok("main|/tmp/app.db\naux|/tmp/aux.db"),
            ..Default::default()
        };
        let theme = palette_for(ThemeId::Default);
        let lines = build_render_lines(&snapshot, theme);

        assert!(lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("main|/tmp/app.db"))
        }));
        assert!(lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("aux|/tmp/aux.db"))
        }));
    }
}
