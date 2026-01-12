pub mod handler;

use crossterm::event::KeyEvent;

#[derive(Clone, Debug)]
pub enum Event {
    Init,
    Key(KeyEvent),
    Resize(u16, u16),
}
