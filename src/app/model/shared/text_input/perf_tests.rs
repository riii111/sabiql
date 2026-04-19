use std::time::Instant;

use super::{TextInputState, next_word_start, previous_word_start};
use crate::app::update::action::CursorMove;

// Snapshot of the pre-cache implementation, kept only for local perf comparison.
#[derive(Clone)]
struct BaselineTextInputState {
    content: String,
    cursor: usize,
    viewport_offset: usize,
}

impl BaselineTextInputState {
    fn new(content: impl Into<String>, cursor: usize) -> Self {
        let content = content.into();
        let char_count = content.chars().count();
        Self {
            content,
            cursor: cursor.min(char_count),
            viewport_offset: 0,
        }
    }

    fn move_cursor(&mut self, movement: CursorMove) {
        match movement {
            CursorMove::Left => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            CursorMove::Right => {
                let len = self.content.chars().count();
                if self.cursor < len {
                    self.cursor += 1;
                }
            }
            CursorMove::Home
            | CursorMove::LineStart
            | CursorMove::BufferStart
            | CursorMove::FirstLine => {
                self.cursor = 0;
            }
            CursorMove::End
            | CursorMove::LineEnd
            | CursorMove::BufferEnd
            | CursorMove::LastLine => {
                self.cursor = self.content.chars().count();
            }
            CursorMove::WordForward => {
                self.cursor = next_word_start(&self.content, self.cursor);
            }
            CursorMove::WordBackward => {
                self.cursor = previous_word_start(&self.content, self.cursor);
            }
            CursorMove::Up
            | CursorMove::Down
            | CursorMove::ViewportTop
            | CursorMove::ViewportMiddle
            | CursorMove::ViewportBottom => {}
        }
    }

    fn update_viewport(&mut self, visible_width: usize) {
        if visible_width == 0 {
            self.viewport_offset = 0;
            return;
        }

        let effective_width = if self.cursor == self.content.chars().count() {
            visible_width.saturating_sub(1)
        } else {
            visible_width
        };

        if effective_width == 0 {
            self.viewport_offset = self.cursor;
            return;
        }

        if self.cursor < self.viewport_offset {
            self.viewport_offset = self.cursor;
        } else if self.cursor >= self.viewport_offset + effective_width {
            self.viewport_offset = self.cursor - effective_width + 1;
        }
    }
}

#[test]
#[ignore = "local-only dev benchmark, not tied to a CI issue"]
#[allow(clippy::print_stderr, reason = "benchmark result output")]
fn bench_cached_char_count_speedup() {
    let content = "x".repeat(4096);
    let iterations = 20_000;

    let mut baseline = BaselineTextInputState::new(content.clone(), 2048);
    let start = Instant::now();
    for _ in 0..iterations {
        baseline.move_cursor(CursorMove::End);
        baseline.update_viewport(80);
        baseline.move_cursor(CursorMove::Home);
        baseline.move_cursor(CursorMove::Right);
        baseline.update_viewport(80);
        std::hint::black_box(baseline.cursor);
        std::hint::black_box(baseline.viewport_offset);
    }
    let baseline_elapsed = start.elapsed();

    let mut cached = TextInputState::new(content, 2048);
    let start = Instant::now();
    for _ in 0..iterations {
        cached.move_cursor(CursorMove::End);
        cached.update_viewport(80);
        cached.move_cursor(CursorMove::Home);
        cached.move_cursor(CursorMove::Right);
        cached.update_viewport(80);
        std::hint::black_box(cached.cursor());
        std::hint::black_box(cached.viewport_offset());
    }
    let cached_elapsed = start.elapsed();

    eprintln!(
        "Baseline: {:?} ({:.1} µs/iter), Cached: {:?} ({:.1} µs/iter), Speedup: {:.2}x",
        baseline_elapsed,
        baseline_elapsed.as_micros() as f64 / iterations as f64,
        cached_elapsed,
        cached_elapsed.as_micros() as f64 / iterations as f64,
        baseline_elapsed.as_secs_f64() / cached_elapsed.as_secs_f64(),
    );
}
