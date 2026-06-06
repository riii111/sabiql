use crate::app::ports::outbound::clipboard::{ClipboardError, ClipboardWriter};

pub struct ArboardClipboard;

impl ClipboardWriter for ArboardClipboard {
    fn copy_text(&self, content: &str) -> Result<(), ClipboardError> {
        copy_text(content)
    }
}

#[cfg(not(target_os = "android"))]
fn copy_text(content: &str) -> Result<(), ClipboardError> {
    arboard::Clipboard::new()
        .and_then(|mut cb| cb.set_text(content))
        .map_err(ClipboardError::backend)
}

#[cfg(target_os = "android")]
fn copy_text(_content: &str) -> Result<(), ClipboardError> {
    Err(ClipboardError::Unavailable("Clipboard unavailable".into()))
}
