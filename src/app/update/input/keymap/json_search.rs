use std::sync::LazyLock;

use crate::ports::inbound::KeyCombo;
use crate::update::action::{Action, DetailTarget, InputTarget};

use super::common::detail_search;
use super::model::{Row, hints, resolve};
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> =
    LazyLock::new(|| detail_search(DetailTarget::JsonDetail, InputTarget::JsonSearch, true));

pub fn json_search_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn json_search_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
