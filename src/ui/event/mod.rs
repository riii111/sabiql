pub mod key_translator;

use crate::app::ports::inbound::{InputEvent, InputKeyCombo};

#[derive(Clone, Debug)]
pub enum Event {
    Init,
    Key(InputKeyCombo),
    Paste(String),
    Resize(u16, u16),
}

impl From<Event> for InputEvent {
    fn from(event: Event) -> Self {
        match event {
            Event::Init => Self::Init,
            Event::Key(combo) => Self::Key(combo),
            Event::Paste(text) => Self::Paste(text),
            Event::Resize(w, h) => Self::Resize(w, h),
        }
    }
}
