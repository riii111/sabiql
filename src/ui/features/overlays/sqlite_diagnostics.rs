use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::model::app_state::AppState;
use crate::app::model::sqlite::diagnostics::{DiagnosticFieldKind, display_rows};
use crate::domain::SqliteDiagnosticsSnapshot;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::primitives::utils::text_utils::wrapped_line_count;
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
        let viewport_width = inner.width.max(1);

        if state.sqlite_diagnostics.is_loading() {
            let lines = vec![
                Line::raw(""),
                Line::from(Span::styled(
                    "Loading diagnostics...",
                    Style::default().fg(theme.semantic.status.warning),
                )),
            ];
            return render_lines(frame, inner, state, lines, viewport_width, viewport_height);
        }

        let Some(snapshot) = state.sqlite_diagnostics.snapshot() else {
            let lines = vec![Line::from(Span::styled(
                "Diagnostics unavailable.",
                Style::default().fg(theme.semantic.status.error),
            ))];
            return render_lines(frame, inner, state, lines, viewport_width, viewport_height);
        };

        let quick_check_override = state
            .sqlite_diagnostics
            .is_quick_check_pending()
            .then_some("Running...");
        let lines = build_render_lines(snapshot, theme, quick_check_override);
        render_lines(frame, inner, state, lines, viewport_width, viewport_height)
    }
}

fn render_lines(
    frame: &mut Frame,
    inner: ratatui::layout::Rect,
    state: &AppState,
    lines: Vec<Line<'static>>,
    viewport_width: u16,
    viewport_height: usize,
) -> SqliteDiagnosticsRenderMetrics {
    let content_line_count = wrapped_content_line_count(&lines, viewport_width) as usize;
    let scroll = state
        .sqlite_diagnostics
        .scroll_offset()
        .min(content_line_count.saturating_sub(viewport_height));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0)),
        inner,
    );
    SqliteDiagnosticsRenderMetrics {
        content_line_count,
        viewport_height,
    }
}

pub fn build_render_lines(
    snapshot: &SqliteDiagnosticsSnapshot,
    theme: &ThemePalette,
    quick_check_override: Option<&str>,
) -> Vec<Line<'static>> {
    let label_style = Style::default()
        .fg(theme.semantic.text.primary)
        .add_modifier(Modifier::BOLD);

    let mut lines = vec![Line::raw("")];
    for row in display_rows(snapshot) {
        let value = if row.kind == DiagnosticFieldKind::QuickCheck {
            quick_check_override.unwrap_or(&row.value)
        } else {
            &row.value
        };
        let value_style =
            if row.kind == DiagnosticFieldKind::QuickCheck && quick_check_override.is_some() {
                Style::default().fg(theme.semantic.status.warning)
            } else {
                field_style(row.kind, snapshot, theme)
            };
        append_field_lines(
            &mut lines,
            row.kind.label(),
            value,
            label_style,
            value_style,
        );
    }
    lines
}

fn lines_to_text(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn wrapped_content_line_count(lines: &[Line<'_>], viewport_width: u16) -> u16 {
    wrapped_line_count(&lines_to_text(lines), viewport_width)
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
            && snapshot.quick_check_is_ok().is_some_and(|is_ok| !is_ok)
        {
            Style::default().fg(theme.semantic.status.error)
        } else if kind == DiagnosticFieldKind::QuickCheck
            && snapshot.quick_check_is_ok().is_some_and(|is_ok| is_ok)
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
        let lines = build_render_lines(&snapshot, theme, None);

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

    #[test]
    fn wrapped_content_line_count_exceeds_logical_lines_for_long_values() {
        let snapshot = SqliteDiagnosticsSnapshot {
            db_file: DiagnosticField::ok(
                "/tmp/very/long/database/path/that/will/wrap/in/a/narrow/viewport.db",
            ),
            ..Default::default()
        };
        let theme = palette_for(ThemeId::Default);
        let lines = build_render_lines(&snapshot, theme, None);
        let logical_lines = lines.len();
        let wrapped_lines = wrapped_content_line_count(&lines, 24) as usize;

        assert!(wrapped_lines > logical_lines);
    }
}
