use std::time::Instant;

use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::theme::ThemePalette;

use crate::app::model::app_state::AppState;
use crate::app::model::connection::error_state::ConnectionErrorState;
use crate::app::update::input::keymap::{HintSurface, KeyContext, connection_error_hints};
use crate::primitives::atoms::key_chip;
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::primitives::utils::text_utils::wrapped_line_count;

pub(crate) struct ConnectionError;

impl ConnectionError {
    pub(crate) fn render(frame: &mut Frame, state: &AppState, now: Instant, theme: &ThemePalette) {
        let error_state = &state.connection_error;
        let Some(error_info) = error_state.error_info() else {
            return;
        };

        let details_expanded = error_state.details_expanded();
        let full_area = frame.area();
        let modal_outer_width = full_area.width * 70 / 100;
        let content_width = modal_outer_width.saturating_sub(4);

        // Fixed overhead: summary(1) + spacer(1) + hint(1) + spacer(1)
        //                 + spacer(1) + actions(1) + borders(2) = 8
        const FIXED_OVERHEAD: u16 = 8;

        let details_height = if details_expanded {
            let detail_text = error_state.masked_details().unwrap_or("");
            let tab_replaced = detail_text.replace('\t', "    ");
            let detail_header = 1u16;
            detail_header + wrapped_line_count(&tab_replaced, content_width)
        } else {
            1
        };

        let terminal_cap = full_area.height.saturating_sub(2);
        let max_height = if details_expanded {
            (full_area.height * 60 / 100).min(terminal_cap).max(9)
        } else {
            terminal_cap.max(9)
        };
        let height = Constraint::Length((FIXED_OVERHEAD + details_height).clamp(9, max_height));

        let context = KeyContext::from_state(state);
        let hints = connection_error_hints(HintSurface::Border, &context)
            .into_iter()
            .map(|hint| (hint.key, hint.short));
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(70),
            height,
            " Connection Error ",
            FooterHintBar::new(hints),
            theme,
        );

        let chunks = Layout::vertical([
            Constraint::Length(1), // Summary
            Constraint::Length(1), // Empty
            Constraint::Length(1), // Hint
            Constraint::Length(1), // Empty
            Constraint::Min(1),    // Details area
            Constraint::Length(1), // Empty before actions
            Constraint::Length(1), // Actions
        ])
        .split(inner);

        Self::render_summary(frame, chunks[0], error_info.summary(), theme);
        Self::render_hint(frame, chunks[2], state, theme);
        Self::render_details_section(
            frame,
            chunks[4],
            error_state,
            details_expanded,
            &context,
            theme,
        );
        Self::render_actions(frame, chunks[6], state, now, theme);
    }

    fn render_summary(frame: &mut Frame, area: Rect, summary: &str, theme: &ThemePalette) {
        let line = Line::from(vec![
            Span::styled("✗ ", Style::default().fg(theme.semantic.status.error)),
            Span::styled(
                summary,
                Style::default()
                    .fg(theme.semantic.status.error)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_hint(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let hint = state
            .connection_error
            .error_info()
            .map_or("", |error_info| error_info.hint());
        let mut spans = vec![
            Span::styled("Hint: ", Style::default().fg(theme.semantic.text.accent)),
            Span::styled(hint, Style::default().fg(theme.semantic.text.secondary)),
        ];
        if state.session.is_service_connection()
            && !state.connection_error.has_destination()
            && let Some(path) = state.service_file_path()
        {
            spans.push(Span::styled(
                format!("  (edit {})", path.display()),
                Style::default().fg(theme.semantic.text.muted),
            ));
        }
        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_details_section(
        frame: &mut Frame,
        area: Rect,
        error_state: &ConnectionErrorState,
        expanded: bool,
        context: &KeyContext,
        theme: &ThemePalette,
    ) {
        if expanded {
            let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

            let toggle_line = Line::from(vec![Span::styled(
                "▼ Details",
                Style::default()
                    .fg(theme.component.navigation.section_header)
                    .add_modifier(Modifier::BOLD),
            )]);
            frame.render_widget(Paragraph::new(toggle_line), chunks[0]);

            if let Some(details) = error_state.masked_details() {
                let lines: Vec<Line> = details
                    .lines()
                    .map(|l| Line::from(l.replace('\t', "    ")))
                    .collect();
                let scroll = error_state.scroll_offset();
                let para = Paragraph::new(lines)
                    .scroll((scroll as u16, 0))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(theme.semantic.text.muted));
                frame.render_widget(para, chunks[1]);
            }
        } else {
            let detail_hint = connection_error_hints(HintSurface::Details, context)
                .first()
                .map_or_default(|hint| format!("(press {} to expand)", hint.key));
            let toggle_line = Line::from(vec![
                Span::styled(
                    "▶ ",
                    Style::default().fg(theme.component.navigation.section_header),
                ),
                Span::styled(
                    "Details ",
                    Style::default().fg(theme.component.navigation.section_header),
                ),
                Span::styled(detail_hint, Style::default().fg(theme.semantic.text.muted)),
            ]);
            frame.render_widget(Paragraph::new(toggle_line), area);
        }
    }

    fn render_actions(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        now: Instant,
        theme: &ThemePalette,
    ) {
        let error_state = &state.connection_error;
        let hints = connection_error_hints(HintSurface::Footer, &KeyContext::from_state(state));
        let mut spans = vec![Span::styled(
            "Actions: ",
            Style::default().fg(theme.semantic.text.muted),
        )];
        for (index, hint) in hints.iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(key_chip(&hint.key, theme));
            spans.push(Span::raw(format!(" {}", hint.short)));
        }

        if error_state.is_copied_visible_at(now) {
            spans.push(Span::raw("   "));
            spans.push(Span::styled(
                "Copied!",
                Style::default()
                    .fg(theme.semantic.status.success)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line), area);
    }
}
