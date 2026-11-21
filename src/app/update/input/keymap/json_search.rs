use std::sync::LazyLock;

use crate::update::action::{DetailTarget, InputTarget};

use super::common::detail_search;
use super::model::Row;

pub(super) static ROWS: LazyLock<Vec<Row>> =
    LazyLock::new(|| detail_search(DetailTarget::JsonDetail, InputTarget::JsonSearch, true));
