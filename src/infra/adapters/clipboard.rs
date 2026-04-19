use crate::app::ports::outbound::{ClipboardError, ClipboardWriter};

pub struct ArboardClipboard;

impl ClipboardWriter for ArboardClipboard {
    fn copy_text(&self, content: &str) -> Result<(), ClipboardError> {
        arboard::Clipboard::new().and_then(|mut cb| cb.set_text(content))?;
        Ok(())
    }
}
