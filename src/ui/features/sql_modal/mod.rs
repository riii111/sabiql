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

use crate::app::model::app_state::AppState;
use crate::app::model::shared::engine_feature_profile::EngineFeatureProfile;
use crate::app::model::shared::settings::KeymapPreset;
use crate::app::model::sql_editor::modal::{SQL_MODAL_HEIGHT_PERCENT, SqlModalStatus, SqlModalTab};
use crate::app::policy::write::sql_risk::AcknowledgeReason;
use crate::app::policy::{FeaturePolicy, FeatureRequirement};
use crate::app::update::input::keybindings::{
    sql_modal, sql_modal_compare, sql_modal_normal, sql_modal_plan, sql_modal_plan_explain,
};
use crate::primitives::molecules::overlay::{centered_rect, render_scrim};
use crate::primitives::molecules::{FooterHintBar, render_modal_with_border_color};
use crate::theme::ThemePalette;

mod completion;
mod editor;
mod status;

pub struct SqlModal;

impl SqlModal {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        now: Instant,
        theme: &ThemePalette,
    ) -> Option<u16> {
        let is_confirming = matches!(
            state.sql_modal.status(),
            SqlModalStatus::ConfirmingHigh { .. } | SqlModalStatus::ConfirmingRisk { .. }
        );
        let engine_feature_profile = state.session.active_engine_feature_profile();
        let feature_policy = FeaturePolicy::new(engine_feature_profile);
        let active_tab =
            engine_feature_profile.normalize_sql_modal_tab(state.sql_modal.active_tab());

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
                    let is_match = input.content() == target_name.as_str();
                    let footer = if is_match {
                        FooterHintBar::new([("Enter", "Execute"), ("Esc", "Back")])
                    } else {
                        FooterHintBar::new([("Esc", "Back")])
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
                SqlModalStatus::ConfirmingRisk { reason, .. } => {
                    let (title, border_color) = match reason {
                        AcknowledgeReason::UnknownRisk => (
                            " SQL \u{2500}\u{2500} \u{26a0} UNKNOWN RISK ",
                            theme.semantic.status.warning,
                        ),
                        AcknowledgeReason::TargetNameUnavailable => (
                            " SQL \u{2500}\u{2500} \u{26a0} HIGH ",
                            theme.semantic.status.error,
                        ),
                        AcknowledgeReason::NonAtomicTransaction => (
                            " SQL \u{2500}\u{2500} \u{26a0} NON-ATOMIC ",
                            theme.semantic.status.warning,
                        ),
                    };
                    render_modal_with_border_color(
                        frame,
                        Constraint::Percentage(80),
                        Constraint::Percentage(SQL_MODAL_HEIGHT_PERCENT),
                        title,
                        FooterHintBar::new([("Enter", "Execute"), ("Esc", "Back")]),
                        border_color,
                        theme,
                    )
                }
                _ => unreachable!(),
            }
        } else {
            let hint = match state.sql_modal.status() {
                SqlModalStatus::Editing => {
                    Self::editing_hint(&feature_policy, state.settings.saved_keymap_preset())
                }
                SqlModalStatus::Running => FooterHintBar::message("Running\u{2026}"),
                SqlModalStatus::ConfirmingAnalyzeHigh {
                    input, target_name, ..
                } => {
                    let is_match = input.content() == target_name.as_str();
                    if is_match {
                        FooterHintBar::new([("Enter", "Confirm"), ("Esc", "Cancel")])
                    } else {
                        FooterHintBar::new([("Esc", "Cancel")])
                    }
                }
                SqlModalStatus::ConfirmingAnalyzeRisk { .. } => {
                    FooterHintBar::new([("Enter", "Execute"), ("Esc", "Cancel")])
                }
                _ => {
                    let compare_can_yank = state.explain.can_yank_compare();
                    Self::border_hint(
                        active_tab,
                        compare_can_yank,
                        engine_feature_profile,
                        &feature_policy,
                        state.settings.saved_keymap_preset(),
                    )
                }
            };
            Self::render_modal_with_tabs(frame, active_tab, hint, engine_feature_profile, theme)
        };

        // Add 1-char horizontal padding for breathing room inside the modal
        let content_area = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };

        let status_height = if matches!(
            state.sql_modal.status(),
            SqlModalStatus::ConfirmingHigh { .. } | SqlModalStatus::ConfirmingRisk { .. }
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
                && state.sql_modal.completion().visible
                && !state.sql_modal.completion().candidates.is_empty()
            {
                completion::render_completion_popup(frame, area, main_area, state, theme);
            }
        } else if active_tab == SqlModalTab::Plan {
            let plan_viewport_height = explain::render(frame, main_area, state, now, theme);
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
        hint: FooterHintBar,
        engine_feature_profile: &EngineFeatureProfile,
        theme: &ThemePalette,
    ) -> (Rect, Rect) {
        let area = centered_rect(
            frame.area(),
            Constraint::Percentage(80),
            Constraint::Percentage(SQL_MODAL_HEIGHT_PERCENT),
        );
        render_scrim(frame, theme);
        frame.render_widget(Clear, area);

        let title = Self::build_title_with_tabs(active_tab, engine_feature_profile, theme);
        let block = Block::default()
            .title(title)
            .title_bottom(hint.line(theme))
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
        engine_feature_profile: &EngineFeatureProfile,
        theme: &ThemePalette,
    ) -> Line<'static> {
        let title_style = theme.modal_title_style();
        let active_style = Style::default()
            .fg(theme.component.navigation.tab_active)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        let inactive_style = Style::default().fg(theme.component.navigation.tab_inactive);

        let style_for = |tab: SqlModalTab| {
            if tab == active_tab {
                active_style
            } else {
                inactive_style
            }
        };
        let supported_tabs = engine_feature_profile.supported_sql_modal_tabs();

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

    fn border_hint(
        tab: SqlModalTab,
        compare_can_yank: bool,
        engine_feature_profile: &EngineFeatureProfile,
        feature_policy: &FeaturePolicy,
        keymap_preset: KeymapPreset,
    ) -> FooterHintBar {
        match tab {
            SqlModalTab::Sql if engine_feature_profile.supported_sql_modal_tabs().len() == 1 => {
                if feature_policy.is_enabled(FeatureRequirement::Explain) {
                    FooterHintBar::new([
                        sql_modal_normal::RUN.as_hint(),
                        sql_modal_plan_explain(keymap_preset).as_hint(),
                        sql_modal_normal::ENTER_INSERT.as_hint(),
                        sql_modal_normal::CLOSE.as_hint(),
                    ])
                } else {
                    FooterHintBar::new([
                        sql_modal_normal::RUN.as_hint(),
                        sql_modal_normal::ENTER_INSERT.as_hint(),
                        sql_modal_normal::CLOSE.as_hint(),
                    ])
                }
            }
            SqlModalTab::Plan => FooterHintBar::new([
                sql_modal_plan::YANK.as_hint(),
                ("Tab/⇧Tab", sql_modal_plan::TAB.as_hint().1),
                sql_modal_plan::CLOSE.as_hint(),
            ]),
            SqlModalTab::Compare => {
                let mut hints = Vec::new();
                if feature_policy.is_enabled(FeatureRequirement::PlanComparison) {
                    hints.push(sql_modal_compare::EDIT_QUERY.as_hint());
                }
                if compare_can_yank {
                    hints.push(sql_modal_compare::YANK.as_hint());
                }
                hints.extend([
                    ("Tab/⇧Tab", sql_modal_compare::TAB.as_hint().1),
                    sql_modal_compare::CLOSE.as_hint(),
                ]);
                FooterHintBar::new(hints)
            }
            SqlModalTab::Sql => {
                if feature_policy.is_enabled(FeatureRequirement::Explain) {
                    FooterHintBar::new([
                        sql_modal_normal::RUN.as_hint(),
                        sql_modal_plan_explain(keymap_preset).as_hint(),
                        sql_modal_normal::ENTER_INSERT.as_hint(),
                        ("Tab/⇧Tab", sql_modal_plan::TAB.as_hint().1),
                        sql_modal_normal::CLOSE.as_hint(),
                    ])
                } else {
                    FooterHintBar::new([
                        sql_modal_normal::RUN.as_hint(),
                        sql_modal_normal::ENTER_INSERT.as_hint(),
                        ("Tab/⇧Tab", sql_modal_plan::TAB.as_hint().1),
                        sql_modal_normal::CLOSE.as_hint(),
                    ])
                }
            }
        }
    }

    fn editing_hint(feature_policy: &FeaturePolicy, keymap_preset: KeymapPreset) -> FooterHintBar {
        match (
            feature_policy.is_enabled(FeatureRequirement::Explain),
            keymap_preset,
        ) {
            (true, KeymapPreset::Default) => FooterHintBar::new([
                sql_modal::RUN.as_hint(),
                sql_modal_plan::EXPLAIN.as_hint(),
                sql_modal::CLEAR.as_hint(),
                sql_modal::QUERY_HISTORY.as_hint(),
                sql_modal::ESC_NORMAL.as_hint(),
            ]),
            (false, KeymapPreset::Default) => FooterHintBar::new([
                sql_modal::RUN.as_hint(),
                sql_modal::CLEAR.as_hint(),
                sql_modal::QUERY_HISTORY.as_hint(),
                sql_modal::ESC_NORMAL.as_hint(),
            ]),
            (_, KeymapPreset::Ide) => FooterHintBar::new([
                sql_modal::RUN.as_hint(),
                sql_modal::CLEAR.as_hint(),
                sql_modal::ESC_NORMAL.as_hint(),
            ]),
        }
    }
}
