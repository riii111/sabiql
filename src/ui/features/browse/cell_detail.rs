use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::flash_timer::FlashId;
use crate::primitives::atoms::{
    CursorKind, apply_yank_flash, set_terminal_cursor, text_cursor_spans_with_kind,
};
use crate::primitives::molecules::{FooterHintBar, render_modal};
use crate::theme::ThemePalette;

pub struct CellDetailRenderMetrics {
    pub visible_rows: usize,
    pub viewport_width: usize,
}

pub struct CellDetail;

impl CellDetail {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) -> Option<CellDetailRenderMetrics> {
        if !state.cell_detail.is_active() {
            return None;
        }

        let title = format!(
            " Cell Detail \u{2500}\u{2500} {}",
            state.cell_detail.column_name()
        );
        let hints = vec![("y", "Copy"), ("/", "Search"), ("Esc", "Close")];
        let (_area, inner) = render_modal(
            frame,
            Constraint::Percentage(80),
            Constraint::Percentage(70),
            &title,
            FooterHintBar::new(hints),
            theme,
        );

        let (content_area, status_area, search_area) = if state.cell_detail.search().is_active() {
            let [content_area, status_area, search_area] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            (content_area, status_area, Some(search_area))
        } else {
            let [content_area, status_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            (content_area, status_area, None)
        };

        Self::render_content(frame, content_area, state, now, theme);
        Self::render_status(frame, status_area, state, theme);
        if let Some(search_area) = search_area {
            Self::render_search(frame, search_area, state, theme);
        }

        Some(CellDetailRenderMetrics {
            visible_rows: content_area.height as usize,
            viewport_width: content_area.width as usize,
        })
    }

    fn render_content(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        now: std::time::Instant,
        theme: &ThemePalette,
    ) {
        let mut lines = content_lines(state, theme);
        let flash_active = state.flash_timers.is_active(FlashId::CellDetail, now);
        apply_yank_flash(&mut lines, flash_active, theme);

        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((state.cell_detail.scroll_offset() as u16, 0))
                .style(Style::default().fg(theme.semantic.text.primary)),
            area,
        );
    }

    fn render_status(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let search = state.cell_detail.search();
        let status = if search.is_active() {
            if search.matches().is_empty() {
                "0/0".to_string()
            } else {
                format!("{}/{}", search.current_match() + 1, search.matches().len())
            }
        } else {
            format!("{} chars", state.cell_detail.content().chars().count())
        };

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                status,
                Style::default().fg(theme.semantic.text.muted),
            ))),
            area,
        );
    }

    fn render_search(frame: &mut Frame, area: Rect, state: &AppState, theme: &ThemePalette) {
        let search = state.cell_detail.search();
        let input = search.input().content();
        let cursor = search.input().cursor();
        let match_info = if search.matches().is_empty() {
            "0/0".to_string()
        } else {
            format!("{}/{}", search.current_match() + 1, search.matches().len())
        };
        let suffix = format!("  {match_info}");
        let visible_width = area
            .width
            .saturating_sub((1 + UnicodeWidthStr::width(suffix.as_str())) as u16)
            as usize;
        let viewport_offset = search_viewport_offset(input, cursor, visible_width);
        let visible_input = slice_chars_fitting_width(input, viewport_offset, visible_width);
        let relative_cursor = cursor.saturating_sub(viewport_offset);

        let mut spans = vec![Span::styled(
            "/",
            Style::default().fg(theme.semantic.text.accent),
        )];
        spans.extend(text_cursor_spans_with_kind(
            &visible_input,
            relative_cursor,
            0,
            visible_input.chars().count(),
            CursorKind::Insert,
            theme,
        ));
        spans.push(Span::styled(
            suffix,
            Style::default().fg(theme.semantic.text.muted),
        ));

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        set_terminal_cursor(frame, area, &visible_input, 0, relative_cursor, 0, 1);
    }
}

fn content_lines(state: &AppState, theme: &ThemePalette) -> Vec<Line<'static>> {
    let content = state.cell_detail.content();
    if content.is_empty() {
        return vec![Line::from(Span::styled(
            "No content",
            Style::default().fg(theme.semantic.text.placeholder),
        ))];
    }

    let current_match = state
        .cell_detail
        .search()
        .matches()
        .get(state.cell_detail.search().current_match())
        .copied();
    let query_len = state.cell_detail.search().input().content().chars().count();
    let mut char_offset = 0;

    content
        .split('\n')
        .map(|line| {
            let line_len = line.chars().count();
            let line_start = char_offset;
            let line_end = line_start + line_len;
            char_offset = line_end + 1;

            match current_match {
                Some(pos) if query_len > 0 && pos >= line_start && pos < line_end => {
                    highlighted_line(line, pos - line_start, query_len, theme)
                }
                _ => Line::from(Span::raw(line.to_string())),
            }
        })
        .collect()
}

fn highlighted_line(
    line: &str,
    match_start: usize,
    match_len: usize,
    theme: &ThemePalette,
) -> Line<'static> {
    let mut before = String::new();
    let mut matched = String::new();
    let mut after = String::new();

    for (idx, ch) in line.chars().enumerate() {
        if idx < match_start {
            before.push(ch);
        } else if idx < match_start + match_len {
            matched.push(ch);
        } else {
            after.push(ch);
        }
    }

    Line::from(vec![
        Span::raw(before),
        Span::styled(
            matched,
            Style::default()
                .fg(theme.semantic.text.primary)
                .bg(theme.semantic.text.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(after),
    ])
}

fn search_viewport_offset(input: &str, cursor: usize, visible_width: usize) -> usize {
    if visible_width == 0 {
        return cursor;
    }

    let chars: Vec<char> = input.chars().collect();
    let mut viewport_offset = 0;
    let mut width_before_cursor = display_width(&chars[..cursor.min(chars.len())]);

    while width_before_cursor >= visible_width && viewport_offset < cursor {
        width_before_cursor =
            width_before_cursor.saturating_sub(char_width(chars[viewport_offset]));
        viewport_offset += 1;
    }

    viewport_offset
}

fn slice_chars_fitting_width(input: &str, start: usize, visible_width: usize) -> String {
    if visible_width == 0 {
        return String::new();
    }

    let mut width = 0;
    let mut visible = String::new();

    for ch in input.chars().skip(start) {
        let ch_width = char_width(ch);
        if width + ch_width > visible_width {
            break;
        }
        width += ch_width;
        visible.push(ch);
    }

    visible
}

fn display_width(chars: &[char]) -> usize {
    chars.iter().map(|&ch| char_width(ch)).sum()
}

fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}
