use ratatui::style::Color;

/// Base color palette
#[allow(dead_code)]
pub struct Palette;

#[allow(dead_code)]
impl Palette {
    pub const CATPPUCCIN_MOCHA: Color = Color::Rgb(0x1e, 0x1e, 0x2e);
    pub const DUCKBONES: Color = Color::Rgb(0x15, 0x19, 0x26);
}

/// Application color theme constants
#[allow(dead_code)]
pub struct Theme;

#[allow(dead_code)]
impl Theme {
    // Modal/Overlay backgrounds
    pub const MODAL_BG: Color = Palette::DUCKBONES;

    // Completion popup
    pub const COMPLETION_SELECTED_BG: Color = Color::Rgb(0x45, 0x47, 0x5a);

    // Table header/alternating row backgrounds
    pub const TABLE_HEADER_BG: Color = Color::Rgb(0x2a, 0x2a, 0x2e);
}
