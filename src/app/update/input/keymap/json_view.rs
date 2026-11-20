use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, DetailTarget, InputTarget, ModalKind, SearchStep};

use super::common::document_navigation;
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface, KeyContext};

fn supported(context: &KeyContext) -> bool {
    context.supports_json_detail()
}

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let view = &[
        HintSurface::Border,
        HintSurface::Current,
        HintSurface::Advanced,
    ];
    let shifted = |action, key| Binding::no_ctrl_alt(action, key);
    let search = |step, key| {
        shifted(
            Action::DetailSearch {
                target: DetailTarget::JsonDetail,
                step,
            },
            Key::Char(key),
        )
    };
    let mut rows = vec![
        Row::new(
            vec![shifted(
                Action::DetailYankAll(DetailTarget::JsonDetail),
                Key::Char('y'),
            )],
            "Copy",
            "Copy full JSON",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::Advanced,
                HintSurface::DataActions,
            ],
        ),
        Row::new(
            vec![
                shifted(Action::JsonEnterEdit, Key::Char('i')),
                shifted(Action::JsonAppendInsert, Key::Char('A')),
            ],
            "Insert",
            "Enter Insert mode / append at line end",
            view,
        ),
        Row::new(
            vec![Binding::action(
                Action::DetailSearch {
                    target: DetailTarget::JsonDetail,
                    step: SearchStep::Enter,
                },
                &[KeyCombo::plain(Key::Char('/'))],
            )],
            "Search",
            "Search JSON text",
            view,
        ),
        Row::new(
            vec![search(SearchStep::Next, 'n'), search(SearchStep::Prev, 'N')],
            "Next/Prev",
            "Jump to next / previous search result",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
    ];
    rows.extend(document_navigation(InputTarget::JsonEdit, true));
    rows.push(Row::new(
        vec![
            Binding::no_ctrl_alt(Action::CloseModal(ModalKind::JsonDetail), Key::Esc)
                .when(supported),
            Binding::action(
                Action::CloseModal(ModalKind::JsonDetail),
                &[KeyCombo::plain(Key::Esc), KeyCombo::plain(Key::Char('q'))],
            ),
        ],
        "Close",
        "Close JSON detail",
        view,
    ));
    rows
});

pub fn json_view_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn json_view_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
