use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::app::sql_modal_context::SqlModalStatus;
use crate::app::state::AppState;
use crate::ui::primitives::molecules::{render_modal, render_modal_with_border_color};
use crate::ui::theme::Theme;

mod completion;
mod cursor;
mod editor;
mod status;

pub struct SqlModal;

impl SqlModal {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let (area, inner) = match state.sql_modal.status() {
            SqlModalStatus::Confirming(decision) => {
                let title = format!(
                    " SQL \u{2500}\u{2500} \u{26a0} {} ",
                    decision.risk_level.as_str()
                );
                render_modal_with_border_color(
                    frame,
                    Constraint::Percentage(80),
                    Constraint::Percentage(60),
                    &title,
                    " Enter: Execute \u{2502} Esc: Back ",
                    Theme::risk_color(decision.risk_level),
                )
            }
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
            SqlModalStatus::Editing => render_modal(
                frame,
                Constraint::Percentage(80),
                Constraint::Percentage(60),
                " SQL Editor ",
                " \u{2325}Enter: Run \u{2502} ^L: Clear \u{2502} ^O: Hist \u{2502} Esc: Normal ",
            ),
            SqlModalStatus::Running => render_modal(
                frame,
                Constraint::Percentage(80),
                Constraint::Percentage(60),
                " SQL Editor ",
                " Running\u{2026} ",
            ),
            SqlModalStatus::Normal | SqlModalStatus::Success | SqlModalStatus::Error => {
                render_modal(
                    frame,
                    Constraint::Percentage(80),
                    Constraint::Percentage(60),
                    " SQL Editor ",
                    " \u{2325}Enter: Run \u{2502} y: Yank \u{2502} ^O: Hist \u{2502} Enter: Insert \u{2502} Esc: Close ",
                )
            }
        };

        let status_height = if matches!(
            state.sql_modal.status(),
            SqlModalStatus::ConfirmingHigh { .. }
        ) {
            3 // warning line + input prompt line + bottom margin
        } else {
            1
        };

        let [editor_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(status_height)]).areas(inner);

        editor::render_editor(frame, editor_area, state);
        status::render_status(frame, status_area, state);

        if matches!(state.sql_modal.status(), SqlModalStatus::Editing)
            && state.sql_modal.completion.visible
            && !state.sql_modal.completion.candidates.is_empty()
        {
            completion::render_completion_popup(frame, area, editor_area, state);
        }
    }
}
