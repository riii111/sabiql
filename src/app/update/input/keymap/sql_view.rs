use std::sync::LazyLock;

use crate::model::sql_editor::modal::SqlModalTab;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ModalKind, ScrollTarget};

use super::common::document_navigation;
use super::model::{Binding, Row, hints};
use super::scroll::line_scroll;
use super::{Hint, HintSurface, KeyContext};

pub(super) static SQL: LazyLock<Vec<Row>> = LazyLock::new(|| rows(SqlModalTab::Sql));
pub(super) static PLAN: LazyLock<Vec<Row>> = LazyLock::new(|| rows(SqlModalTab::Plan));
pub(super) static COMPARE: LazyLock<Vec<Row>> = LazyLock::new(|| rows(SqlModalTab::Compare));

fn rows(tab: SqlModalTab) -> Vec<Row> {
    let sql = tab == SqlModalTab::Sql;
    let mut rows = Vec::new();
    if sql {
        rows.push(Row::new(
            vec![
                Binding::with_alt(Action::SqlModalSubmit, Key::Enter),
                Binding::action(Action::SqlModalSubmit, &[KeyCombo::plain(Key::F(5))]),
            ],
            "Run",
            "Execute query",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Editing,
            ],
        ));
    }
    rows.push(Row::new(
        vec![
            Binding::action(Action::ExplainRequest, &[KeyCombo::ctrl(Key::Char('e'))])
                .ide(&[KeyCombo::plain(Key::Char('E'))]),
        ],
        "Explain",
        "Run EXPLAIN on current query",
        if sql {
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
                HintSurface::ExplainPlaceholder,
            ]
        } else {
            &[
                HintSurface::Current,
                HintSurface::Advanced,
                HintSurface::ExplainPlaceholder,
            ]
        },
    ));
    rows.push(Row::new(
        vec![Binding::with_alt(
            Action::ExplainAnalyzeRequest,
            Key::Char('e'),
        )],
        "Analyze",
        "Run EXPLAIN ANALYZE on current query",
        &[HintSurface::Current, HintSurface::Advanced],
    ));
    if tab == SqlModalTab::Compare {
        rows.push(Row::new(
            vec![Binding::action(
                Action::CompareEditQuery,
                &[KeyCombo::plain(Key::Char('e'))],
            )],
            "Edit",
            "Edit query in SQL tab",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ));
    }
    rows.push(Row {
        bindings: vec![Binding::no_ctrl_alt(Action::SqlModalYank, Key::Char('y'))],
        short: "Yank",
        description: if sql {
            "Copy query to clipboard"
        } else {
            "Copy to clipboard"
        },
        references: &[],
        dynamic_label: None,
        visible: if tab == SqlModalTab::Compare {
            Some(|context, surface| {
                surface != HintSurface::Border || context.sql_compare_can_yank()
            })
        } else {
            None
        },
        surfaces: if sql {
            &[HintSurface::Current, HintSurface::Editing]
        } else {
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
            ]
        },
    });
    if sql {
        rows.push(Row::new(
            vec![
                Binding::no_ctrl_alt(Action::SqlModalEnterInsert, Key::Char('i')),
                Binding::no_ctrl_alt(Action::SqlModalAppendInsert, Key::Char('A')),
            ],
            "Insert",
            "Enter Insert mode / append at line end",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Editing,
            ],
        ));
    }
    rows.push(Row {
        bindings: vec![
            Binding::action(Action::SqlModalNextTab, &[KeyCombo::plain(Key::Tab)]),
            Binding::no_ctrl_alt(Action::SqlModalPrevTab, Key::BackTab),
        ],
        short: "Switch",
        description: "Switch tab",
        references: &[],
        dynamic_label: None,
        visible: Some(|context, surface| {
            surface != HintSurface::Border || context.sql_has_multiple_tabs()
        }),
        surfaces: &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Advanced,
        ],
    });
    if sql {
        rows.push(Row::new(
            vec![Binding::with_ctrl(Action::SqlModalClear, Key::Char('l'))],
            "Clear",
            "Clear editor",
            &[HintSurface::Current, HintSurface::Editing],
        ));
        rows.push(Row::new(
            vec![
                Binding::action(
                    Action::OpenModal(ModalKind::QueryHistoryPicker),
                    &[KeyCombo::ctrl(Key::Char('o'))],
                )
                .ide(&[KeyCombo::plain(Key::Char('O'))])
                .when(KeyContext::can_open_history),
            ],
            "History",
            "Open Query History",
            &[HintSurface::Current, HintSurface::Editing],
        ));
        let mut navigation = document_navigation(InputTarget::SqlModal, false);
        for row in &mut navigation {
            row.surfaces = &[HintSurface::Current, HintSurface::Editing];
        }
        rows.extend(navigation);
    } else {
        let target = if tab == SqlModalTab::Plan {
            ScrollTarget::ExplainPlan
        } else {
            ScrollTarget::ExplainCompare
        };
        let mut scroll = line_scroll(target);
        scroll.description = if tab == SqlModalTab::Plan {
            "Scroll plan text"
        } else {
            "Scroll comparison text"
        };
        scroll.surfaces = &[HintSurface::Current, HintSurface::Advanced];
        rows.push(scroll);
    }
    rows.push(Row::new(
        vec![Binding::no_ctrl_alt(
            Action::CloseModal(ModalKind::SqlModal),
            Key::Esc,
        )],
        "Close",
        "Close editor",
        &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Advanced,
            HintSurface::Editing,
        ],
    ));
    rows
}

pub(super) fn active_rows(context: &KeyContext) -> Option<&'static [Row]> {
    Some(match context.sql_tab()? {
        SqlModalTab::Sql => &SQL,
        SqlModalTab::Plan => &PLAN,
        SqlModalTab::Compare => &COMPARE,
    })
}

pub fn sql_view_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    active_rows(context).map_or_else(Vec::new, |rows| hints(rows, context, surface))
}
