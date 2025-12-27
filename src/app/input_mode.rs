#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum InputMode {
    #[default]
    Normal,
    CommandLine,
    Filter,
}
