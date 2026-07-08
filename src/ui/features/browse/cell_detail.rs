use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use unicode_casefold::UnicodeCaseFold;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::detail_view::DetailDisplayMode;
use crate::app::model::shared::flash_timer::FlashId;
use crate::features::browse::detail_view::{render_detail_search, search_match_status};
use crate::primitives::atoms::apply_yank_flash;
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
            search_match_status(search)
        } else if state.cell_detail.display_mode() == DetailDisplayMode::FormattedJson {
            format!(
                "Formatted JSON | {} chars raw",
                state.cell_detail.original_content().chars().count()
            )
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
        render_detail_search(frame, area, state.cell_detail.search(), theme);
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
    let query = state.cell_detail.search().input().content().to_string();
    let mut char_offset = 0;

    content
        .split('\n')
        .map(|line| {
            let line_len = line.chars().count();
            let line_start = char_offset;
            let line_end = line_start + line_len;
            char_offset = line_end + 1;

            match current_match {
                Some(pos) if !query.is_empty() && pos >= line_start && pos < line_end => {
                    highlighted_line(line, pos - line_start, &query, theme)
                }
                _ => Line::from(Span::raw(line.to_string())),
            }
        })
        .collect()
}

fn highlighted_line(
    line: &str,
    match_start: usize,
    query: &str,
    theme: &ThemePalette,
) -> Line<'static> {
    let match_len = folded_match_len(line, match_start, query);
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

fn folded_match_len(line: &str, match_start: usize, query: &str) -> usize {
    let target_len = query.case_fold().collect::<String>().chars().count();
    let mut folded_len = 0;
    let mut original_len = 0;

    for ch in line.chars().skip(match_start) {
        folded_len += ch.case_fold().count();
        original_len += 1;
        if folded_len >= target_len {
            break;
        }
    }

    original_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_match_len_keeps_sharp_s_to_one_original_char() {
        assert_eq!(folded_match_len("straße", 4, "ss"), 1);
    }

    #[test]
    fn folded_match_len_keeps_ascii_query_length() {
        assert_eq!(folded_match_len("alphabet", 2, "pha"), 3);
    }
}
