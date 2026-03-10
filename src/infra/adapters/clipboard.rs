use crate::app::ports::ClipboardWriter;

pub struct ArboardClipboard;

impl ClipboardWriter for ArboardClipboard {
    fn copy_text(&self, content: &str) -> Result<(), String> {
        arboard::Clipboard::new()
            .and_then(|mut cb| cb.set_text(content))
            .map_err(|e| e.to_string())
    }
}
