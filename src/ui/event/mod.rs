pub mod handler;
pub mod key_translator;

use crate::app::keybindings::KeyCombo;

#[derive(Clone, Debug)]
pub enum Event {
    Init,
    Key(KeyCombo),
    Paste(String),
    Resize(u16, u16),
}
