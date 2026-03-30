mod compare;
mod explain;
mod plan_highlight;

use std::sync::LazyLock;
use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::model::app_state::AppState;
use crate::app::model::sql_editor::modal::{SqlModalStatus, SqlModalTab};
use crate::app::update::input::keybindings::{
    SQL_MODAL_COMPARE_KEYS, SQL_MODAL_KEYS, SQL_MODAL_NORMAL_KEYS, SQL_MODAL_PLAN_KEYS, idx,
};
use crate::ui::primitives::molecules::overlay::{centered_rect, render_scrim};
use crate::ui::primitives::molecules::render_modal_with_border_color;
use crate::ui::theme::Theme;

mod completion;
mod editor;
mod status;

pub struct SqlModal;

impl SqlModal {
    pub fn render(frame: &mut Frame, state: &AppState, now: Instant) -> Option<u16> {
        let is_confirming = matches!(
            state.sql_modal.status(),
            SqlModalStatus::ConfirmingHigh { .. }
        );

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
                        Constraint::Percentage(60),
                        &title,
                        footer,
                        Theme::STATUS_ERROR,
                    )
                }
                _ => unreachable!(),
            }
        } else {
            let hint: &str = match state.sql_modal.status() {
                SqlModalStatus::Editing => Self::editing_hint(),
                SqlModalStatus::Running => " Running\u{2026} ",
                SqlModalStatus::ConfirmingAnalyzeHigh {
                    input, target_name, ..
                } => {
                    let is_match = target_name
                        .as_ref()
                        .is_some_and(|name| input.content() == name);
                    if is_match {
                        " Enter: Confirm \u{2502} Esc: Cancel "
                    } else {
                        " Esc: Cancel "
                    }
                }
                _ => {
                    let compare_can_yank =
                        state.explain.left.is_some() && state.explain.right.is_some();
                    Self::border_hint(state.sql_modal.active_tab, compare_can_yank)
                }
            };
            Self::render_modal_with_tabs(frame, state.sql_modal.active_tab, hint)
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
            Paragraph::new(Line::styled(
                sep_line,
                Style::default().fg(Theme::MODAL_BORDER),
            )),
            separator_area,
        );

        if is_confirming || state.sql_modal.active_tab == SqlModalTab::Sql {
            editor::render_editor(frame, main_area, state, now);
            status::render_status(frame, status_area, state);

            if matches!(state.sql_modal.status(), SqlModalStatus::Editing)
                && state.sql_modal.completion.visible
                && !state.sql_modal.completion.candidates.is_empty()
            {
                completion::render_completion_popup(frame, area, main_area, state);
            }
        } else if state.sql_modal.active_tab == SqlModalTab::Plan {
            let compare_viewport_height = explain::render(frame, main_area, state, now);
            status::render_status(frame, status_area, state);
            return Some(compare_viewport_height);
        } else {
            let compare_viewport_height = compare::render(frame, main_area, state, now);
            status::render_status(frame, status_area, state);
            return Some(compare_viewport_height);
        }

        None
    }

    fn render_modal_with_tabs(
        frame: &mut Frame,
        active_tab: SqlModalTab,
        hint: &str,
    ) -> (Rect, Rect) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(80),
            Constraint::Percentage(60),
        );
        render_scrim(frame);
        frame.render_widget(Clear, area);

        let title = Self::build_title_with_tabs(active_tab);
        let block = Block::default()
            .title(title)
            .title_bottom(Line::styled(
                hint.to_string(),
                Style::default()
                    .fg(Theme::MODAL_TITLE)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(Theme::MODAL_BORDER))
            .style(Style::default());
        let inner = block.inner(area);
        frame.render_widget(block, area);

        (area, inner)
    }

    fn build_title_with_tabs(active_tab: SqlModalTab) -> Line<'static> {
        let title_style = Style::default()
            .fg(Theme::MODAL_TITLE)
            .add_modifier(Modifier::BOLD);
        let active_style = Style::default()
            .fg(Theme::TAB_ACTIVE)
            .add_modifier(Modifier::BOLD);
        let inactive_style = Style::default().fg(Theme::TAB_INACTIVE);

        let style_for = |tab: SqlModalTab| {
            if tab == active_tab {
                active_style
            } else {
                inactive_style
            }
        };

        Line::from(vec![
            Span::styled(" SQL Editor ", title_style),
            Span::styled(
                "\u{2500}\u{2500} ",
                Style::default().fg(Theme::MODAL_BORDER),
            ),
            Span::styled("[SQL]", style_for(SqlModalTab::Sql)),
            Span::raw(" "),
            Span::styled("[Plan]", style_for(SqlModalTab::Plan)),
            Span::raw(" "),
            Span::styled("[Compare]", style_for(SqlModalTab::Compare)),
            Span::raw(" "),
        ])
    }

    fn border_hint(tab: SqlModalTab, compare_can_yank: bool) -> &'static str {
        static PLAN: LazyLock<String> = LazyLock::new(|| {
            SqlModal::join_hint_pairs(&[
                SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::YANK].as_hint(),
                SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::SCROLL].as_hint(),
                (
                    "Tab/⇧Tab",
                    SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::TAB].as_hint().1,
                ),
                SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::CLOSE].as_hint(),
            ])
        });
        static COMPARE_WITH_YANK: LazyLock<String> = LazyLock::new(|| {
            SqlModal::join_hint_pairs(&[
                SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::EDIT_QUERY].as_hint(),
                SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::YANK].as_hint(),
                SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::SCROLL].as_hint(),
                (
                    "Tab/⇧Tab",
                    SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::TAB]
                        .as_hint()
                        .1,
                ),
                SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::CLOSE].as_hint(),
            ])
        });
        static COMPARE_NO_YANK: LazyLock<String> = LazyLock::new(|| {
            SqlModal::join_hint_pairs(&[
                SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::EDIT_QUERY].as_hint(),
                SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::SCROLL].as_hint(),
                (
                    "Tab/⇧Tab",
                    SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::TAB]
                        .as_hint()
                        .1,
                ),
                SQL_MODAL_COMPARE_KEYS[idx::sql_modal_compare::CLOSE].as_hint(),
            ])
        });
        static SQL: LazyLock<String> = LazyLock::new(|| {
            SqlModal::join_hint_pairs(&[
                SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::RUN].as_hint(),
                SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::EXPLAIN].as_hint(),
                SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::ANALYZE].as_hint(),
                SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::YANK].as_hint(),
                SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::QUERY_HISTORY].as_hint(),
                SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::ENTER_INSERT].as_hint(),
                (
                    "Tab/⇧Tab",
                    SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::TAB].as_hint().1,
                ),
                SQL_MODAL_NORMAL_KEYS[idx::sql_modal_normal::CLOSE].as_hint(),
            ])
        });
        match tab {
            SqlModalTab::Plan => &PLAN,
            SqlModalTab::Compare if compare_can_yank => &COMPARE_WITH_YANK,
            SqlModalTab::Compare => &COMPARE_NO_YANK,
            SqlModalTab::Sql => &SQL,
        }
    }

    fn editing_hint() -> &'static str {
        static HINT: LazyLock<String> = LazyLock::new(|| {
            SqlModal::join_hint_pairs(&[
                SQL_MODAL_KEYS[idx::sql_modal::RUN].as_hint(),
                SQL_MODAL_PLAN_KEYS[idx::sql_modal_plan::EXPLAIN].as_hint(),
                SQL_MODAL_KEYS[idx::sql_modal::CLEAR].as_hint(),
                SQL_MODAL_KEYS[idx::sql_modal::QUERY_HISTORY].as_hint(),
                SQL_MODAL_KEYS[idx::sql_modal::ESC_NORMAL].as_hint(),
            ])
        });
        &HINT
    }

    fn join_hint_pairs(pairs: &[(&str, &str)]) -> String {
        let parts: Vec<String> = pairs
            .iter()
            .map(|(key, desc)| format!("{key}: {desc}"))
            .collect();
        format!(" {} ", parts.join(" \u{2502} "))
    }
}
