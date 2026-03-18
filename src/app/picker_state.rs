#[derive(Debug, Clone, Default)]
pub struct PickerState {
    pub selected: usize,
    pub scroll_offset: usize,
    pub pane_height: u16,
    pub filter_input: String,
}

impl PickerState {
    pub fn visible_items(&self) -> usize {
        self.pane_height as usize
    }

    pub fn set_selection(&mut self, index: usize) {
        self.scroll_offset = clamp_scroll_offset(index, self.scroll_offset, self.visible_items());
        self.selected = index;
    }

    pub fn reset(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }
}

pub fn clamp_scroll_offset(selected: usize, current_offset: usize, viewport: usize) -> usize {
    if viewport == 0 {
        return 0;
    }
    let bottom_edge = current_offset + viewport.saturating_sub(1);
    if selected > bottom_edge {
        selected - viewport.saturating_sub(1)
    } else if selected < current_offset {
        selected
    } else {
        current_offset
    }
}
