const SPINNER_FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];

pub fn spinner_char(time_ms: u128) -> &'static str {
    SPINNER_FRAMES[(time_ms / 300) as usize % SPINNER_FRAMES.len()]
}
