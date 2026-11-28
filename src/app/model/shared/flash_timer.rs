use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::motion::FRAME_INTERVAL;

const FLASH_DURATION: Duration = Duration::from_millis(200);
// Background colors cannot fade on a terminal of unknown color, so the copied text alone keeps the flash color and fades back.
pub const FLASH_AFTERGLOW: Duration = Duration::from_millis(180);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlashPhase {
    Off,
    Solid,
    /// Progress back to the normal text color; 0.0 is still the flash color.
    Afterglow(f32),
}

/// The afterglow is reported only when `animated`, so static rendering keeps the plain on/off flash.
pub fn flash_phase(until: Instant, now: Instant, animated: bool) -> FlashPhase {
    if now < until {
        return FlashPhase::Solid;
    }
    let elapsed = now.saturating_duration_since(until);
    if animated && elapsed < FLASH_AFTERGLOW {
        FlashPhase::Afterglow(elapsed.as_secs_f32() / FLASH_AFTERGLOW.as_secs_f32())
    } else {
        FlashPhase::Off
    }
}

pub fn afterglow_frame(until: Instant, now: Instant) -> Option<Instant> {
    let ends_at = until + FLASH_AFTERGLOW;
    (until <= now && now < ends_at).then(|| (now + FRAME_INTERVAL).min(ends_at))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlashId {
    SqlModal,
    Ddl,
    JsonDetail,
    CellDetail,
    RowDetail,
}

#[derive(Debug, Clone, Default)]
pub struct FlashTimerStore {
    timers: HashMap<FlashId, Instant>,
}

impl FlashTimerStore {
    pub fn set(&mut self, id: FlashId, now: Instant) {
        self.timers.insert(id, now + FLASH_DURATION);
    }

    pub fn is_active(&self, id: FlashId, now: Instant) -> bool {
        self.timers.get(&id).is_some_and(|&until| now < until)
    }

    pub fn phase(&self, id: FlashId, now: Instant, animated: bool) -> FlashPhase {
        self.timers
            .get(&id)
            .map_or(FlashPhase::Off, |&until| flash_phase(until, now, animated))
    }

    pub fn next_afterglow_frame(&self, now: Instant) -> Option<Instant> {
        self.timers
            .values()
            .filter_map(|&until| afterglow_frame(until, now))
            .min()
    }

    pub fn clear(&mut self, id: FlashId) {
        self.timers.remove(&id);
    }

    pub fn clear_expired(&mut self, now: Instant) {
        self.timers
            .retain(|_, until| now < *until + FLASH_AFTERGLOW);
    }

    /// The next end of a solid flash; flashes already in their afterglow are scheduled by `next_afterglow_frame`.
    pub fn earliest_deadline(&self, now: Instant) -> Option<Instant> {
        self.timers
            .values()
            .copied()
            .filter(|&until| until > now)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_is_active() {
        let mut store = FlashTimerStore::default();
        let now = Instant::now();

        store.set(FlashId::Ddl, now);

        assert!(store.is_active(FlashId::Ddl, now));
        assert!(!store.is_active(FlashId::SqlModal, now));
    }

    #[test]
    fn expires_after_duration() {
        let mut store = FlashTimerStore::default();
        let now = Instant::now();

        store.set(FlashId::Ddl, now);
        let after = now + FLASH_DURATION;

        assert!(!store.is_active(FlashId::Ddl, after));
    }

    #[test]
    fn clear_removes_timer() {
        let mut store = FlashTimerStore::default();
        let now = Instant::now();

        store.set(FlashId::SqlModal, now);
        store.clear(FlashId::SqlModal);

        assert!(!store.is_active(FlashId::SqlModal, now));
    }

    #[test]
    fn clear_expired_removes_only_expired() {
        let mut store = FlashTimerStore::default();
        let now = Instant::now();

        store.set(FlashId::Ddl, now);
        store.set(FlashId::SqlModal, now);

        let after = now + FLASH_DURATION + FLASH_AFTERGLOW;
        store.clear_expired(after);

        assert!(!store.is_active(FlashId::Ddl, now));
        assert!(!store.is_active(FlashId::SqlModal, now));
    }

    #[test]
    fn earliest_deadline_returns_min() {
        let mut store = FlashTimerStore::default();
        let now = Instant::now();
        let later = now + Duration::from_millis(50);

        store.set(FlashId::Ddl, now);
        store.set(FlashId::SqlModal, later);

        let earliest = store.earliest_deadline(now).unwrap();
        assert_eq!(earliest, now + FLASH_DURATION);
    }

    #[test]
    fn clear_expired_keeps_fading_timers() {
        let mut store = FlashTimerStore::default();
        let now = Instant::now();
        store.set(FlashId::Ddl, now);

        let fading = now + FLASH_DURATION;
        store.clear_expired(fading);

        assert_eq!(
            store.phase(FlashId::Ddl, fading, true),
            FlashPhase::Afterglow(0.0)
        );
    }

    #[test]
    fn phase_moves_from_solid_through_afterglow_to_off() {
        let now = Instant::now();
        let until = now + FLASH_DURATION;

        assert_eq!(flash_phase(until, now, true), FlashPhase::Solid);
        assert_eq!(flash_phase(until, until, true), FlashPhase::Afterglow(0.0));
        assert_eq!(
            flash_phase(until, until + FLASH_AFTERGLOW, true),
            FlashPhase::Off
        );
    }

    #[test]
    fn static_rendering_skips_afterglow() {
        let now = Instant::now();

        assert_eq!(flash_phase(now, now, false), FlashPhase::Off);
    }

    #[test]
    fn afterglow_requests_frames_until_it_ends() {
        let now = Instant::now();

        assert_eq!(afterglow_frame(now, now), Some(now + FRAME_INTERVAL));
        assert_eq!(
            afterglow_frame(
                now,
                now + FLASH_AFTERGLOW.saturating_sub(Duration::from_millis(1))
            ),
            Some(now + FLASH_AFTERGLOW)
        );
        assert_eq!(afterglow_frame(now + FLASH_DURATION, now), None);
    }

    #[test]
    fn earliest_deadline_skips_flashes_already_fading() {
        let mut store = FlashTimerStore::default();
        let now = Instant::now();
        store.set(FlashId::Ddl, now);

        assert_eq!(store.earliest_deadline(now + FLASH_DURATION), None);
    }

    #[test]
    fn earliest_deadline_empty_returns_none() {
        let store = FlashTimerStore::default();

        assert!(store.earliest_deadline(Instant::now()).is_none());
    }
}
