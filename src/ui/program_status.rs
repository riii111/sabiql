use std::time::{Duration, Instant};

use crate::app::model::app_state::AppState;
use crate::domain::QueryResult;

// Most queries finish before anyone looks away, so only longer work is announced to keep host notifications quiet.
const REPORT_AFTER: Duration = Duration::from_secs(1);

/// States sent with the Program Status Protocol (OSC 7501), which hosts such as Rex and herdr show per pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramState {
    Idle,
    Working,
    Done,
    Error,
}

impl ProgramState {
    fn wire_value(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Error => "error",
        }
    }

    pub fn sequence(self) -> String {
        format!("\x1b]7501;state={}:app=sabiql\x1b\\", self.wire_value())
    }
}

pub const CLEAR_SEQUENCE: &str = "\x1b]7501;state=clear\x1b\\";

#[derive(Debug, Clone, Copy)]
struct BusyRun {
    since: Instant,
    result_generation: u64,
}

#[derive(Debug, Default)]
pub struct ProgramStatusReporter {
    reported: Option<ProgramState>,
    busy: Option<BusyRun>,
}

impl ProgramStatusReporter {
    /// The state to announce after drawing `state`, when it differs from the last announcement.
    pub fn observe(&mut self, state: &AppState, now: Instant) -> Option<ProgramState> {
        let next = if is_busy(state) {
            let run = *self.busy.get_or_insert_with(|| BusyRun {
                since: now,
                result_generation: state.query.result_generation(),
            });
            (now.saturating_duration_since(run.since) >= REPORT_AFTER)
                .then_some(ProgramState::Working)
        } else {
            let run = self.busy.take();
            (self.reported == Some(ProgramState::Working)).then(|| finished_state(state, run))
        }?;
        self.announce(next)
    }

    pub fn acknowledge(&mut self) -> Option<ProgramState> {
        match self.reported {
            // Input after a finished run means the user has seen the outcome.
            Some(ProgramState::Done | ProgramState::Error) => self.announce(ProgramState::Idle),
            Some(ProgramState::Idle | ProgramState::Working) | None => None,
        }
    }

    /// Whether a status was announced and still needs `CLEAR_SEQUENCE`; resets the reporter.
    pub fn take_reported(&mut self) -> bool {
        self.busy = None;
        self.reported.take().is_some()
    }

    fn announce(&mut self, next: ProgramState) -> Option<ProgramState> {
        (self.reported != Some(next)).then(|| {
            self.reported = Some(next);
            next
        })
    }
}

fn is_busy(state: &AppState) -> bool {
    // ER preparation hands off between steps without a frame that stays busy, so only query runs are reported.
    state.query.is_running()
}

fn finished_state(state: &AppState, run: Option<BusyRun>) -> ProgramState {
    // An error result left over from an earlier query must not mark a later, unrelated run as failed.
    let produced_result =
        run.is_none_or(|run| run.result_generation != state.query.result_generation());
    let failed_query = produced_result
        && state
            .query
            .visible_result()
            .is_some_and(QueryResult::is_error);
    if failed_query || state.messages.last_error().is_some() {
        ProgramState::Error
    } else {
        ProgramState::Done
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::domain::QuerySource;

    fn running_state(now: Instant) -> AppState {
        let mut state = AppState::new("test".to_string());
        let _ = state.query.begin_running(now);
        state
    }

    fn report_working(reporter: &mut ProgramStatusReporter, state: &AppState, now: Instant) {
        assert_eq!(reporter.observe(state, now), None);
        assert_eq!(
            reporter.observe(state, now + REPORT_AFTER),
            Some(ProgramState::Working)
        );
    }

    #[test]
    fn sequence_uses_colon_separated_fields() {
        assert_eq!(
            ProgramState::Working.sequence(),
            "\x1b]7501;state=working:app=sabiql\x1b\\"
        );
    }

    #[test]
    fn short_run_is_never_announced() {
        let now = Instant::now();
        let mut state = running_state(now);
        let mut reporter = ProgramStatusReporter::default();

        assert_eq!(reporter.observe(&state, now), None);
        state.query.mark_idle();

        assert_eq!(reporter.observe(&state, now), None);
    }

    #[test]
    fn long_run_is_announced_once() {
        let now = Instant::now();
        let state = running_state(now);
        let mut reporter = ProgramStatusReporter::default();

        report_working(&mut reporter, &state, now);

        assert_eq!(reporter.observe(&state, now + REPORT_AFTER * 2), None);
    }

    #[test]
    fn successful_finish_is_done() {
        let now = Instant::now();
        let mut state = running_state(now);
        let mut reporter = ProgramStatusReporter::default();
        report_working(&mut reporter, &state, now);

        state.query.mark_idle();

        assert_eq!(
            reporter.observe(&state, now + REPORT_AFTER),
            Some(ProgramState::Done)
        );
    }

    #[test]
    fn failed_query_is_error() {
        let now = Instant::now();
        let mut state = running_state(now);
        let mut reporter = ProgramStatusReporter::default();
        report_working(&mut reporter, &state, now);

        state.query.mark_idle();
        state.query.set_current_result(Arc::new(QueryResult::error(
            "SELECT 1".to_string(),
            "boom".to_string(),
            1,
            QuerySource::Adhoc,
        )));

        assert_eq!(
            reporter.observe(&state, now + REPORT_AFTER),
            Some(ProgramState::Error)
        );
    }

    #[test]
    fn earlier_error_result_does_not_fail_later_run() {
        let now = Instant::now();
        let mut state = AppState::new("test".to_string());
        state.query.set_current_result(Arc::new(QueryResult::error(
            "SELECT 1".to_string(),
            "boom".to_string(),
            1,
            QuerySource::Adhoc,
        )));
        let _ = state.query.begin_running(now);
        let mut reporter = ProgramStatusReporter::default();
        report_working(&mut reporter, &state, now);

        state.query.mark_idle();

        assert_eq!(
            reporter.observe(&state, now + REPORT_AFTER),
            Some(ProgramState::Done)
        );
    }

    #[test]
    fn acknowledge_after_finish_returns_to_idle() {
        let now = Instant::now();
        let mut state = running_state(now);
        let mut reporter = ProgramStatusReporter::default();
        report_working(&mut reporter, &state, now);
        state.query.mark_idle();
        let _ = reporter.observe(&state, now + REPORT_AFTER);

        assert_eq!(reporter.acknowledge(), Some(ProgramState::Idle));
        assert_eq!(reporter.acknowledge(), None);
    }

    #[test]
    fn acknowledge_while_working_keeps_working() {
        let now = Instant::now();
        let state = running_state(now);
        let mut reporter = ProgramStatusReporter::default();
        report_working(&mut reporter, &state, now);

        assert_eq!(reporter.acknowledge(), None);
    }

    #[test]
    fn take_reported_only_after_an_announcement() {
        let now = Instant::now();
        let state = running_state(now);
        let mut reporter = ProgramStatusReporter::default();
        assert!(!reporter.take_reported());

        report_working(&mut reporter, &state, now);

        assert!(reporter.take_reported());
        assert!(!reporter.take_reported());
    }
}
