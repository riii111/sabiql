use std::fmt::Write as _;
use std::time::Instant;

use crate::cmd::effect::Effect;
use crate::domain::explain_plan::{ComparisonVerdict, compare_plans};
use crate::model::app_state::AppState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::text_input::TextInputLike;
use crate::model::sql_editor::modal::SqlModalTab;
use crate::ports::outbound::ClipboardError;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub(super) fn reduce_yank(
    state: &mut AppState,
    action: &Action,
    now: Instant,
    services: &AppServices,
) -> DispatchResult {
    match action {
        Action::SqlModalYank => {
            let active_tab = services
                .db_capabilities
                .normalize_sql_modal_tab(state.sql_modal.active_tab());
            let content = match active_tab {
                SqlModalTab::Plan => state.explain.plan_text.clone(),
                SqlModalTab::Compare => match (&state.explain.left, &state.explain.right) {
                    (Some(l), Some(r)) => {
                        let result = compare_plans(&l.plan, &r.plan);
                        let verdict = match result.verdict {
                            ComparisonVerdict::Improved => "Improved",
                            ComparisonVerdict::Worsened => "Worsened",
                            ComparisonVerdict::Similar => "Similar",
                            ComparisonVerdict::Unavailable => "Unavailable",
                        };
                        let mut verdict_section = verdict.to_string();
                        for reason in &result.reasons {
                            let _ = write!(verdict_section, "\n  • {reason}");
                        }

                        let mut sections = vec![verdict_section];
                        for (pos, s) in [("Left", l), ("Right", r)] {
                            let mode = if s.plan.is_analyze {
                                "ANALYZE"
                            } else {
                                "EXPLAIN"
                            };
                            sections.push(format!(
                                "--- {}: {} ({}, {:.2}s) ---\n{}",
                                pos,
                                s.source.label(),
                                mode,
                                s.plan.execution_secs(),
                                s.plan.raw_text
                            ));
                        }
                        Some(sections.join("\n\n"))
                    }
                    _ => None,
                },
                SqlModalTab::Sql => {
                    if state.sql_modal.editor.content().is_empty() {
                        None
                    } else {
                        Some(state.sql_modal.editor.content().to_string())
                    }
                }
            };
            match content {
                Some(c) if !c.is_empty() => {
                    DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                        content: c,
                        on_success: Some(Box::new(Action::SqlModalYankSuccess)),
                        on_failure: Some(Box::new(Action::CopyFailed(
                            ClipboardError::Unavailable("Clipboard unavailable".into()),
                        ))),
                    }])
                }
                _ => DispatchResult::handled(),
            }
        }
        Action::SqlModalYankSuccess => {
            state.flash_timers.set(FlashId::SqlModal, now);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}
