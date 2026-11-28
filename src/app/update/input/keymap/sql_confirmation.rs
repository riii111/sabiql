use std::sync::LazyLock;

use crate::ports::inbound::Key;
use crate::update::action::{Action, InputTarget, ScrollAmount, ScrollDirection, ScrollTarget};

use super::common::confirmation_text_field;
use super::context::SqlConfirmation;
use super::model::{Binding, Row, hints};
use super::{Hint, HintSurface, KeyContext};

pub(super) static HIGH: LazyLock<Vec<Row>> = LazyLock::new(|| rows(false, true));
pub(super) static RISK: LazyLock<Vec<Row>> = LazyLock::new(|| rows(false, false));
pub(super) static ANALYZE_HIGH: LazyLock<Vec<Row>> = LazyLock::new(|| rows(true, true));
pub(super) static ANALYZE_RISK: LazyLock<Vec<Row>> = LazyLock::new(|| rows(true, false));

fn rows(analyze: bool, typed: bool) -> Vec<Row> {
    let (confirm, cancel, confirm_label, cancel_label) = if analyze {
        (
            Action::ExplainAnalyzeConfirm,
            Action::ExplainAnalyzeCancel,
            if typed { "Confirm" } else { "Execute" },
            "Cancel",
        )
    } else {
        (
            Action::SqlModalConfirmExecute,
            Action::SqlModalCancelConfirm,
            "Execute",
            "Back",
        )
    };
    let mut rows = vec![
        Row {
            bindings: vec![Binding::no_ctrl_alt(confirm, Key::Enter)],
            short: confirm_label,
            description: "Execute the confirmed statement",
            references: &[],
            dynamic_label: None,
            visible: Some(|context, surface| {
                surface != HintSurface::Border || context.is_sql_confirmation_matched()
            }),
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Editing,
            ],
        },
        Row::new(
            vec![Binding::any_modifiers(cancel, Key::Esc)],
            cancel_label,
            "Cancel and return to editor",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Editing,
            ],
        ),
    ];
    if analyze {
        rows.push(Row::new(
            vec![
                Binding::no_ctrl_alt(
                    Action::Scroll {
                        target: ScrollTarget::ExplainConfirm,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::Line,
                    },
                    Key::Down,
                ),
                Binding::no_ctrl_alt(
                    Action::Scroll {
                        target: ScrollTarget::ExplainConfirm,
                        direction: ScrollDirection::Up,
                        amount: ScrollAmount::Line,
                    },
                    Key::Up,
                ),
            ],
            "Scroll",
            "Scroll confirmation",
            &[HintSurface::Current, HintSurface::Navigation],
        ));
    }
    if typed {
        let target = if analyze {
            InputTarget::SqlModalAnalyzeHighRisk
        } else {
            InputTarget::SqlModalHighRisk
        };
        rows.extend(confirmation_text_field(target));
        rows.push(Row::new(
            vec![Binding::text(target)],
            "Type",
            "Type confirmation name",
            &[HintSurface::Current, HintSurface::Editing],
        ));
    }
    rows
}

pub(super) fn active_rows(context: &KeyContext) -> Option<&'static [Row]> {
    Some(match context.sql_confirmation()? {
        SqlConfirmation::High => &HIGH,
        SqlConfirmation::Risk => &RISK,
        SqlConfirmation::AnalyzeHigh => &ANALYZE_HIGH,
        SqlConfirmation::AnalyzeRisk => &ANALYZE_RISK,
    })
}

pub fn sql_confirmation_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    active_rows(context).map_or_else(Vec::new, |rows| hints(rows, context, surface))
}
