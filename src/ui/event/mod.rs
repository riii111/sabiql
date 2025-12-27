pub mod handler;

use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum Event {
    Init,
    Tick,
    Render,
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Quit,
}
