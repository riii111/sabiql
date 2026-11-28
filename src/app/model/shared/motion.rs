use std::time::{Duration, Instant};

use super::focused_pane::FocusedPane;

const SCROLL_DURATION: Duration = Duration::from_millis(140);
const FOCUS_DURATION: Duration = Duration::from_millis(160);
const MODAL_DURATION: Duration = Duration::from_millis(140);
const MODAL_CLOSE_DURATION: Duration = Duration::from_millis(100);

// About 60 fps; terminals redraw only changed cells, so short tweens stay cheap.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrolledPane {
    Result,
    Inspector,
    Help,
}

#[derive(Debug, Clone, Copy)]
struct Tween {
    started_at: Instant,
    duration: Duration,
}

impl Tween {
    fn new(now: Instant, duration: Duration) -> Self {
        Self {
            started_at: now,
            duration,
        }
    }

    fn ends_at(self) -> Instant {
        self.started_at + self.duration
    }

    fn is_active(self, now: Instant) -> bool {
        now < self.ends_at()
    }

    fn eased_progress(self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started_at);
        let linear = (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        ease_out_cubic(linear)
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[derive(Debug, Clone, Copy)]
struct ScrollTween {
    pane: ScrolledPane,
    from: usize,
    to: usize,
    tween: Tween,
}

impl ScrollTween {
    fn offset_at(self, now: Instant) -> usize {
        let progress = self.tween.eased_progress(now);
        let from = self.from as f32;
        let to = self.to as f32;
        (to - from).mul_add(progress, from).round() as usize
    }
}

#[derive(Debug, Clone, Copy)]
struct FocusTween {
    from: FocusedPane,
    to: FocusedPane,
    tween: Tween,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusTransition {
    pub from: FocusedPane,
    pub to: FocusedPane,
    pub progress: f32,
}

#[derive(Debug, Clone, Default)]
pub struct MotionState {
    enabled: bool,
    last_frame: Option<Instant>,
    scroll: Option<ScrollTween>,
    focus: Option<FocusTween>,
    modal: Option<Tween>,
    modal_close: Option<Tween>,
}

impl MotionState {
    pub fn note_frame(&mut self, now: Instant) {
        self.last_frame = Some(now);
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn begin_scroll(&mut self, pane: ScrolledPane, from: usize, to: usize, now: Instant) {
        if !self.enabled || from == to {
            return;
        }
        self.scroll = Some(ScrollTween {
            pane,
            from,
            to,
            tween: Tween::new(now, SCROLL_DURATION),
        });
    }

    pub fn scroll_target(&self) -> Option<(ScrolledPane, usize)> {
        self.scroll.map(|scroll| (scroll.pane, scroll.to))
    }

    pub fn cancel_scroll(&mut self) {
        self.scroll = None;
    }

    /// A tween applies only while the model still targets its end offset; any other change wins immediately.
    pub fn scroll_offset(&self, pane: ScrolledPane, actual: usize, now: Instant) -> usize {
        match self.scroll {
            Some(scroll)
                if scroll.pane == pane && scroll.to == actual && scroll.tween.is_active(now) =>
            {
                scroll.offset_at(now)
            }
            _ => actual,
        }
    }

    pub fn begin_focus(&mut self, from: FocusedPane, to: FocusedPane, now: Instant) {
        if !self.enabled || from == to {
            return;
        }
        self.focus = Some(FocusTween {
            from,
            to,
            tween: Tween::new(now, FOCUS_DURATION),
        });
    }

    pub fn focus_transition(&self, now: Instant) -> Option<FocusTransition> {
        self.focus
            .filter(|focus| focus.tween.is_active(now))
            .map(|focus| FocusTransition {
                from: focus.from,
                to: focus.to,
                progress: focus.tween.eased_progress(now),
            })
    }

    pub fn begin_modal(&mut self, now: Instant) {
        if self.enabled {
            self.modal = Some(Tween::new(now, MODAL_DURATION));
        }
    }

    pub fn modal_progress(&self, now: Instant) -> Option<f32> {
        self.modal
            .filter(|modal| modal.is_active(now))
            .map(|modal| modal.eased_progress(now))
    }

    pub fn begin_modal_close(&mut self, now: Instant) {
        if self.enabled {
            self.modal = None;
            self.modal_close = Some(Tween::new(now, MODAL_CLOSE_DURATION));
        }
    }

    /// How dim the panes still are after a modal closed, falling from 1.0 to 0.0.
    pub fn modal_close_dim(&self, now: Instant) -> Option<f32> {
        self.modal_close
            .filter(|close| close.is_active(now))
            .map(|close| 1.0 - close.eased_progress(now))
    }

    pub fn next_frame(&self, now: Instant) -> Option<Instant> {
        [
            self.scroll.map(|scroll| scroll.tween),
            self.focus.map(|focus| focus.tween),
            self.modal,
            self.modal_close,
        ]
        .into_iter()
        .flatten()
        // A tween that ended without a frame drawn at its end still owes that final frame.
        .filter(|tween| {
            tween.is_active(now) || self.last_frame.is_none_or(|drawn| drawn < tween.ends_at())
        })
        .map(|tween| (now + FRAME_INTERVAL).min(tween.ends_at()))
        .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> MotionState {
        let mut motion = MotionState::default();
        motion.enable();
        motion
    }

    mod scroll {
        use super::*;

        #[test]
        fn disabled_motion_draws_actual_offset() {
            let mut motion = MotionState::default();
            let now = Instant::now();

            motion.begin_scroll(ScrolledPane::Result, 0, 40, now);

            assert_eq!(motion.scroll_offset(ScrolledPane::Result, 40, now), 40);
        }

        #[test]
        fn starts_at_previous_offset_and_settles_on_target() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_scroll(ScrolledPane::Result, 0, 40, now);

            assert_eq!(motion.scroll_offset(ScrolledPane::Result, 40, now), 0);
            assert_eq!(
                motion.scroll_offset(ScrolledPane::Result, 40, now + SCROLL_DURATION),
                40
            );
        }

        #[test]
        fn intermediate_frame_lies_between_endpoints() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_scroll(ScrolledPane::Result, 0, 40, now);
            let offset = motion.scroll_offset(ScrolledPane::Result, 40, now + SCROLL_DURATION / 2);

            assert!(offset > 0 && offset < 40, "offset was {offset}");
        }

        #[test]
        fn scrolling_up_interpolates_downward_offsets() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_scroll(ScrolledPane::Inspector, 30, 0, now);
            let offset =
                motion.scroll_offset(ScrolledPane::Inspector, 0, now + SCROLL_DURATION / 2);

            assert!(offset > 0 && offset < 30, "offset was {offset}");
        }

        #[test]
        fn other_pane_keeps_actual_offset() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_scroll(ScrolledPane::Result, 0, 40, now);

            assert_eq!(motion.scroll_offset(ScrolledPane::Inspector, 7, now), 7);
        }

        #[test]
        fn offset_changed_elsewhere_snaps_to_actual() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_scroll(ScrolledPane::Result, 0, 40, now);

            assert_eq!(motion.scroll_offset(ScrolledPane::Result, 41, now), 41);
        }
    }

    mod focus {
        use super::*;

        #[test]
        fn transition_reports_panes_and_progress() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_focus(FocusedPane::Explorer, FocusedPane::Result, now);

            assert_eq!(
                motion.focus_transition(now),
                Some(FocusTransition {
                    from: FocusedPane::Explorer,
                    to: FocusedPane::Result,
                    progress: 0.0,
                })
            );
        }

        #[test]
        fn transition_ends_after_duration() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_focus(FocusedPane::Explorer, FocusedPane::Result, now);

            assert_eq!(motion.focus_transition(now + FOCUS_DURATION), None);
        }

        #[test]
        fn same_pane_starts_no_transition() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_focus(FocusedPane::Result, FocusedPane::Result, now);

            assert_eq!(motion.focus_transition(now), None);
        }
    }

    mod modal {
        use super::*;

        #[test]
        fn progress_runs_until_duration() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_modal(now);

            assert_eq!(motion.modal_progress(now), Some(0.0));
            assert_eq!(motion.modal_progress(now + MODAL_DURATION), None);
        }
    }

    mod modal_close {
        use super::*;

        #[test]
        fn dim_lifts_from_full_to_none() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_modal_close(now);

            assert_eq!(motion.modal_close_dim(now), Some(1.0));
            assert_eq!(motion.modal_close_dim(now + MODAL_CLOSE_DURATION), None);
        }

        #[test]
        fn closing_cancels_an_unfinished_fade_in() {
            let mut motion = enabled();
            let now = Instant::now();
            motion.begin_modal(now);

            motion.begin_modal_close(now);

            assert_eq!(motion.modal_progress(now), None);
        }

        #[test]
        fn disabled_motion_closes_at_once() {
            let mut motion = MotionState::default();
            let now = Instant::now();

            motion.begin_modal_close(now);

            assert_eq!(motion.modal_close_dim(now), None);
        }
    }

    mod scheduling {
        use super::*;

        #[test]
        fn idle_motion_has_no_frame() {
            let motion = enabled();

            assert_eq!(motion.next_frame(Instant::now()), None);
        }

        #[test]
        fn active_tween_requests_next_frame() {
            let mut motion = enabled();
            let now = Instant::now();

            motion.begin_modal(now);

            assert_eq!(motion.next_frame(now), Some(now + FRAME_INTERVAL));
        }

        #[test]
        fn ended_tween_without_final_frame_requests_it() {
            let mut motion = enabled();
            let now = Instant::now();
            motion.begin_modal(now);
            motion.note_frame(now + MODAL_DURATION / 2);
            let late = now + MODAL_DURATION * 2;

            assert_eq!(motion.next_frame(late), Some(now + MODAL_DURATION));

            motion.note_frame(late);

            assert_eq!(motion.next_frame(late), None);
        }

        #[test]
        fn last_frame_lands_on_tween_end() {
            let mut motion = enabled();
            let now = Instant::now();
            motion.begin_modal(now);
            let near_end = now + MODAL_DURATION.saturating_sub(Duration::from_millis(1));

            assert_eq!(motion.next_frame(near_end), Some(now + MODAL_DURATION));
        }
    }
}
