#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMove {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
    WordForward,
    WordBackward,
    ReadlineWordStart,
    ReadlineWordEnd,
    FirstLine,
    LastLine,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
}
