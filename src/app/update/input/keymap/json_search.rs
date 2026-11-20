use std::sync::LazyLock;

use crate::update::action::{DetailTarget, InputTarget};

use super::common::detail_search;
use super::model::{Row, hints};
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> =
    LazyLock::new(|| detail_search(DetailTarget::JsonDetail, InputTarget::JsonSearch, true));

pub fn json_search_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
