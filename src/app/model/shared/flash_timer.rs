use std::collections::HashMap;
use std::time::{Duration, Instant};

const FLASH_DURATION: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlashId {
    SqlModal,
    Ddl,
    JsonbDetail,
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

    pub fn clear(&mut self, id: FlashId) {
        self.timers.remove(&id);
    }

    pub fn clear_expired(&mut self, now: Instant) {
        self.timers.retain(|_, until| now < *until);
    }

    pub fn earliest_deadline(&self) -> Option<Instant> {
        self.timers.values().copied().min()
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
    fn earliest_deadline_empty_returns_none() {
        let store = FlashTimerStore::default();

        assert!(store.earliest_deadline().is_none());
    }
}
