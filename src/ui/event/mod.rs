pub mod handler;

use crossterm::event::KeyEvent;

#[derive(Clone, Debug)]
pub enum Event {
    Init,
    Tick,
    Key(KeyEvent),
    Resize(u16, u16),
}
