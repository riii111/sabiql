pub mod handler;
pub mod key_translator;

use crossterm::event::KeyEvent;

#[derive(Clone, Debug)]
pub enum Event {
    Init,
    Key(KeyEvent),
    Paste(String),
    Resize(u16, u16),
}
