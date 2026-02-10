pub mod handler;

use crossterm::event::KeyEvent;

#[derive(Clone, Debug)]
pub enum Event {
    Init,
    Key(KeyEvent),
    Paste(String),
    Resize(u16, u16),
}
