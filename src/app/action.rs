#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Action {
    None,
    Quit,
    Tick,
    Render,
    Resize(u16, u16),
    SwitchToBrowse,
    SwitchToER,
    ToggleFocus,
    Up,
    Down,
    Left,
    Right,
}
