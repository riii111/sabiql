use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::ui_state::YankFlash;

const FLASH_DURATION: Duration = Duration::from_millis(200);

/// Remove a timed value at its deadline and report whether it expired.
pub(crate) fn clear_expired<T>(
    value: &mut Option<T>,
    now: Instant,
    deadline: impl FnOnce(&T) -> Instant,
) -> bool {
    if value.as_ref().is_some_and(|value| now >= deadline(value)) {
        *value = None;
        true
    } else {
        false
    }
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
    result: Option<YankFlash>,
}

impl FlashTimerStore {
    pub fn set(&mut self, id: FlashId, now: Instant) {
        self.timers.insert(id, now + FLASH_DURATION);
    }

    pub fn set_result(&mut self, row: usize, col: Option<usize>, now: Instant) {
        self.result = Some(YankFlash {
            row,
            col,
            until: now + FLASH_DURATION,
        });
    }

    pub fn result(&self) -> Option<YankFlash> {
        self.result
    }

    pub fn is_active(&self, id: FlashId, now: Instant) -> bool {
        self.timers.get(&id).is_some_and(|&until| now < until)
    }

    pub fn clear(&mut self, id: FlashId) {
        self.timers.remove(&id);
    }

    pub fn clear_expired(&mut self, now: Instant) {
        self.timers.retain(|_, until| now < *until);
        clear_expired(&mut self.result, now, |flash| flash.until);
    }

    pub fn earliest_deadline(&self) -> Option<Instant> {
        self.timers
            .values()
            .copied()
            .chain(self.result.map(|flash| flash.until))
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

        let after = now + FLASH_DURATION;
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

        let earliest = store.earliest_deadline().unwrap();
        assert_eq!(earliest, now + FLASH_DURATION);
    }

    #[test]
    fn result_flash_shares_deadline_and_expiry_with_panel_flashes() {
        let mut store = FlashTimerStore::default();
        let now = Instant::now();
        store.set_result(3, Some(2), now);
        store.set(FlashId::Ddl, now + Duration::from_millis(50));
        assert_eq!(store.earliest_deadline(), Some(now + FLASH_DURATION));
        store.clear_expired(now + Duration::from_millis(50));
        assert!(store.result().is_some());
        store.clear_expired(now + FLASH_DURATION);
        assert!(store.result().is_none());
        assert!(store.is_active(FlashId::Ddl, now + FLASH_DURATION));
    }

    #[test]
    fn earliest_deadline_empty_returns_none() {
        let store = FlashTimerStore::default();

        assert!(store.earliest_deadline().is_none());
    }
}
