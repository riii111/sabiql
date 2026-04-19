mod compare;
mod explain;
mod plan_highlight;

use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::catalog::sql_modal_border_hint;
use crate::app::model::app_state::AppState;
use crate::app::model::sql_editor::modal::{SQL_MODAL_HEIGHT_PERCENT, SqlModalStatus, SqlModalTab};
use crate::app::services::AppServices;
use crate::ui::primitives::molecules::overlay::{centered_rect, render_scrim};
use crate::ui::primitives::molecules::render_modal_with_border_color;
use crate::ui::theme::ThemePalette;

mod completion;
mod editor;
mod status;

pub struct SqlModal;

impl SqlModal {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        services: &AppServices,
        now: Instant,
        theme: &ThemePalette,
    ) -> Option<u16> {
        let is_confirming = matches!(
            state.sql_modal.status(),
            SqlModalStatus::ConfirmingHigh { .. }
        );
        let active_tab = services
            .db_capabilities
            .normalize_sql_modal_tab(state.sql_modal.active_tab);

        let (area, inner) = if is_confirming {
            match state.sql_modal.status() {
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
                        Constraint::Percentage(SQL_MODAL_HEIGHT_PERCENT),
                        &title,
                        footer,
                        theme.semantic.status.error,
                        theme,
                    )
                }
                _ => unreachable!(),
            }
        } else {
            let hint = match state.sql_modal.status() {
                SqlModalStatus::Running => " Running\u{2026} ".to_owned(),
                SqlModalStatus::ConfirmingAnalyzeHigh {
                    input, target_name, ..
                } => {
                    let is_match = target_name
                        .as_ref()
                        .is_some_and(|name| input.content() == name);
                    if is_match {
                        " Enter: Confirm \u{2502} Esc: Cancel ".to_owned()
                    } else {
                        " Esc: Cancel ".to_owned()
                    }
                }
                _ => sql_modal_border_hint(state, active_tab, services),
            };
            Self::render_modal_with_tabs(frame, active_tab, &hint, services, theme)
        };

        // Add 1-char horizontal padding for breathing room inside the modal
        let content_area = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };

        let status_height = if matches!(
            state.sql_modal.status(),
            SqlModalStatus::ConfirmingHigh { .. }
        ) {
            3
        } else {
            1
        };

        let [main_area, separator_area, status_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(status_height),
        ])
        .areas(content_area);

        // Draw horizontal separator between editor and status bar
        let sep_line = "\u{2500}".repeat(separator_area.width as usize);
        frame.render_widget(
            Paragraph::new(Line::styled(sep_line, theme.modal_border_style())),
            separator_area,
        );

        if is_confirming || active_tab == SqlModalTab::Sql {
            editor::render_editor(frame, main_area, state, now, theme);
            status::render_status(frame, status_area, state, theme);

            if matches!(state.sql_modal.status(), SqlModalStatus::Editing)
                && state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty()
            {
                completion::render_completion_popup(frame, area, main_area, state, theme);
            }
        } else if active_tab == SqlModalTab::Plan {
            let plan_viewport_height =
                explain::render(frame, main_area, state, services, now, theme);
            status::render_status(frame, status_area, state, theme);
            return Some(plan_viewport_height);
        } else {
            let compare_viewport_height = compare::render(frame, main_area, state, now, theme);
            status::render_status(frame, status_area, state, theme);
            return Some(compare_viewport_height);
        }

        None
    }

    fn render_modal_with_tabs(
        frame: &mut Frame,
        active_tab: SqlModalTab,
        hint: &str,
        services: &AppServices,
        theme: &ThemePalette,
    ) -> (Rect, Rect) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(80),
            Constraint::Percentage(SQL_MODAL_HEIGHT_PERCENT),
        );
        render_scrim(frame, theme);
        frame.render_widget(Clear, area);

        let title = Self::build_title_with_tabs(active_tab, services, theme);
        let block = Block::default()
            .title(title)
            .title_bottom(Line::styled(hint.to_string(), theme.modal_hint_style()))
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(theme.modal_border_style())
            .style(Style::default().fg(theme.semantic.text.primary));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        (area, inner)
    }

    fn build_title_with_tabs(
        active_tab: SqlModalTab,
        services: &AppServices,
        theme: &ThemePalette,
    ) -> Line<'static> {
        let title_style = theme.modal_title_style();
        let active_style = Style::default()
            .fg(theme.component.navigation.tab_active)
            .add_modifier(Modifier::BOLD);
        let inactive_style = Style::default().fg(theme.component.navigation.tab_inactive);

        let style_for = |tab: SqlModalTab| {
            if tab == active_tab {
                active_style
            } else {
                inactive_style
            }
        };
        let supported_tabs = services.db_capabilities.supported_sql_modal_tabs();

        if supported_tabs.len() == 1 {
            return Line::from(vec![Span::styled(" SQL Editor ", title_style)]);
        }

        let mut spans = vec![
            Span::styled(" SQL Editor ", title_style),
            Span::styled("\u{2500}\u{2500} ", theme.modal_border_style()),
        ];
        for tab in supported_tabs {
            let label = match tab {
                SqlModalTab::Sql => "[SQL]",
                SqlModalTab::Plan => "[Plan]",
                SqlModalTab::Compare => "[Compare]",
            };
            spans.push(Span::styled(label, style_for(*tab)));
            spans.push(Span::raw(" "));
        }
        Line::from(spans)
    }
}
