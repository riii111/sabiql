mod icon;
mod key_chip;
mod panel_border;
pub(crate) mod scroll_indicator;
mod spinner;
mod sql_highlight;
pub(crate) mod status_message;
mod text_cursor;
mod yank_flash;

pub(crate) use icon::Icon;
pub(crate) use key_chip::{key_chip, key_text};
pub(crate) use panel_border::{panel_block, panel_block_highlight};
pub(crate) use spinner::spinner_char;
pub(crate) use sql_highlight::{highlight_sql, highlight_sql_spans};
#[cfg(test)]
pub(crate) use text_cursor::insert_cursor_span_with_kind;
pub(crate) use text_cursor::{
    CursorKind, ModalTextSurface, build_modal_text_surface_lines, render_modal_text_surface,
    set_terminal_cursor, text_cursor_spans, text_cursor_spans_with_kind,
};
pub(crate) use yank_flash::{apply_yank_flash, apply_yank_flash_masked};
