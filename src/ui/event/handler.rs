use crossterm::event::KeyCode;

use crate::app::action::Action;
use crate::app::state::AppState;

use super::Event;

pub fn handle_event(event: Event, _state: &AppState) -> Action {
    match event {
        Event::Quit => Action::Quit,
        Event::Render => Action::Render,
        Event::Resize(w, h) => Action::Resize(w, h),
        Event::Key(key) => match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('1') => Action::SwitchToBrowse,
            KeyCode::Char('2') => Action::SwitchToER,
            KeyCode::Char('f') => Action::ToggleFocus,
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::Left | KeyCode::Char('h') => Action::Left,
            KeyCode::Right | KeyCode::Char('l') => Action::Right,
            _ => Action::None,
        },
        _ => Action::None,
    }
}
