mod diff;
mod display;
mod handling;

pub(crate) use diff::{
    normalize_for_write_diff, normalize_structured_json_for_write, uses_structured_json_diff,
};
pub(crate) use display::format_for_cell_detail;
pub(crate) use handling::CellPresentationPolicy;
