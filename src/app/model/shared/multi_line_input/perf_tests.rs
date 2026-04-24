use std::time::Instant;

use super::{MultiLineInputState, next_word_start, previous_word_start};
use crate::update::action::CursorMove;

// Snapshot of the pre-cache implementation, kept only for local perf comparison.
#[derive(Clone)]
struct BaselineTextInputState {
    content: String,
    cursor: usize,
}

impl BaselineTextInputState {
    fn new(content: impl Into<String>, cursor: usize) -> Self {
        let content = content.into();
        let char_count = content.chars().count();
        Self {
            content,
            cursor: cursor.min(char_count),
        }
    }

    fn content(&self) -> &str {
        &self.content
    }

    fn cursor(&self) -> usize {
        self.cursor
    }

    fn char_count(&self) -> usize {
        self.content.chars().count()
    }

    fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.char_count());
    }

    fn move_cursor(&mut self, movement: CursorMove) {
        match movement {
            CursorMove::Left => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            CursorMove::Right => {
                if self.cursor < self.char_count() {
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
                self.cursor = self.char_count();
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
}

// Snapshot of the pre-cache multi-line implementation, including its uncached inner text input.
#[derive(Clone)]
struct BaselineMultiLineInputState {
    inner: BaselineTextInputState,
    scroll_row: usize,
    preferred_col: Option<usize>,
}

impl BaselineMultiLineInputState {
    fn new(content: impl Into<String>, cursor: usize) -> Self {
        Self {
            inner: BaselineTextInputState::new(content, cursor),
            scroll_row: 0,
            preferred_col: None,
        }
    }

    fn content(&self) -> &str {
        self.inner.content()
    }

    fn cursor(&self) -> usize {
        self.inner.cursor()
    }

    fn line_spans(&self) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        let mut start = 0;
        for line in self.content().split('\n') {
            let len = line.chars().count();
            result.push((start, len));
            start += len + 1;
        }
        result
    }

    fn current_line_col(&self) -> (usize, usize) {
        let cursor = self.cursor();
        let lines = self.line_spans();
        for (i, (start, len)) in lines.iter().enumerate() {
            if cursor >= *start && cursor <= start + len {
                return (i, cursor - start);
            }
        }
        (0, cursor)
    }

    fn set_cursor_raw(&mut self, pos: usize) {
        self.inner.set_cursor(pos.min(self.inner.char_count()));
    }

    fn move_cursor(&mut self, movement: CursorMove) {
        match movement {
            CursorMove::Left | CursorMove::Right => {
                self.inner.move_cursor(movement);
                self.preferred_col = None;
            }
            CursorMove::Up => {
                let (current_line, current_col) = self.current_line_col();
                let preferred_col = self.preferred_col.unwrap_or(current_col);
                if current_line > 0 {
                    let lines = self.line_spans();
                    let (prev_start, prev_len) = lines[current_line - 1];
                    self.set_cursor_raw(prev_start + preferred_col.min(prev_len));
                }
                self.preferred_col = Some(preferred_col);
            }
            CursorMove::Down => {
                let (current_line, current_col) = self.current_line_col();
                let preferred_col = self.preferred_col.unwrap_or(current_col);
                let lines = self.line_spans();
                if current_line + 1 < lines.len() {
                    let (next_start, next_len) = lines[current_line + 1];
                    self.set_cursor_raw(next_start + preferred_col.min(next_len));
                }
                self.preferred_col = Some(preferred_col);
            }
            CursorMove::Home | CursorMove::LineStart => {
                let (current_line, _) = self.current_line_col();
                let lines = self.line_spans();
                if let Some((start, _)) = lines.get(current_line) {
                    self.set_cursor_raw(*start);
                }
                self.preferred_col = None;
            }
            CursorMove::End | CursorMove::LineEnd => {
                let (current_line, _) = self.current_line_col();
                let lines = self.line_spans();
                if let Some((start, len)) = lines.get(current_line) {
                    self.set_cursor_raw(start + len);
                }
                self.preferred_col = None;
            }
            CursorMove::WordForward => {
                self.set_cursor_raw(next_word_start(self.content(), self.cursor()));
                self.preferred_col = None;
            }
            CursorMove::WordBackward => {
                self.set_cursor_raw(previous_word_start(self.content(), self.cursor()));
                self.preferred_col = None;
            }
            CursorMove::BufferStart => {
                self.set_cursor_raw(0);
                self.preferred_col = None;
            }
            CursorMove::BufferEnd => {
                self.set_cursor_raw(self.inner.char_count());
                self.preferred_col = None;
            }
            CursorMove::FirstLine => {
                let (_, current_col) = self.current_line_col();
                let preferred_col = self.preferred_col.unwrap_or(current_col);
                let lines = self.line_spans();
                if let Some((start, len)) = lines.first() {
                    self.set_cursor_raw(start + preferred_col.min(*len));
                }
                self.preferred_col = Some(preferred_col);
            }
            CursorMove::LastLine => {
                let (_, current_col) = self.current_line_col();
                let preferred_col = self.preferred_col.unwrap_or(current_col);
                let lines = self.line_spans();
                if let Some((start, len)) = lines.last() {
                    self.set_cursor_raw(start + preferred_col.min(*len));
                }
                self.preferred_col = Some(preferred_col);
            }
            CursorMove::ViewportTop | CursorMove::ViewportMiddle | CursorMove::ViewportBottom => {}
        }
    }

    fn cursor_to_position(&self) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;

        for (current_pos, ch) in self.content().chars().enumerate() {
            if current_pos >= self.cursor() {
                break;
            }
            if ch == '\n' {
                row += 1;
                col = 0;
            } else {
                col += 1;
            }
        }

        (row, col)
    }

    fn update_scroll(&mut self, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }
        let (row, _) = self.cursor_to_position();
        if row < self.scroll_row {
            self.scroll_row = row;
        } else if row >= self.scroll_row + visible_rows {
            self.scroll_row = row - visible_rows + 1;
        }
    }
}

#[test]
#[ignore = "local-only dev benchmark, not tied to a CI issue"]
#[allow(clippy::print_stderr, reason = "benchmark result output")]
fn bench_multiline_cache_speedup() {
    let content = (0..400)
        .map(|i| format!("line_{i:04}_{}", "x".repeat((i % 17) + 8)))
        .collect::<Vec<_>>()
        .join("\n");
    let iterations = 5_000;

    let mut baseline = BaselineMultiLineInputState::new(content.clone(), 0);
    let start = Instant::now();
    for _ in 0..iterations {
        baseline.move_cursor(CursorMove::LastLine);
        baseline.update_scroll(12);
        std::hint::black_box(baseline.cursor_to_position());
        baseline.move_cursor(CursorMove::FirstLine);
        baseline.update_scroll(12);
        std::hint::black_box(baseline.cursor_to_position());
        baseline.move_cursor(CursorMove::Down);
        baseline.move_cursor(CursorMove::Down);
        baseline.move_cursor(CursorMove::Up);
    }
    let baseline_elapsed = start.elapsed();

    let mut cached = MultiLineInputState::new(content, 0);
    let start = Instant::now();
    for _ in 0..iterations {
        cached.move_cursor(CursorMove::LastLine);
        cached.update_scroll(12);
        std::hint::black_box(cached.cursor_to_position());
        cached.move_cursor(CursorMove::FirstLine);
        cached.update_scroll(12);
        std::hint::black_box(cached.cursor_to_position());
        cached.move_cursor(CursorMove::Down);
        cached.move_cursor(CursorMove::Down);
        cached.move_cursor(CursorMove::Up);
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
