pub trait ClipboardWriter: Send + Sync {
    fn copy_text(&self, content: &str) -> Result<(), String>;
}
