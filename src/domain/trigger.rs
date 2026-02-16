use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

impl fmt::Display for TriggerTiming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Before => write!(f, "BEFORE"),
            Self::After => write!(f, "AFTER"),
            Self::InsteadOf => write!(f, "INSTEAD OF"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
    Truncate,
}

impl fmt::Display for TriggerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Insert => write!(f, "INSERT"),
            Self::Update => write!(f, "UPDATE"),
            Self::Delete => write!(f, "DELETE"),
            Self::Truncate => write!(f, "TRUNCATE"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Trigger {
    pub name: String,
    pub timing: TriggerTiming,
    pub events: Vec<TriggerEvent>,
    pub function_name: String,
    pub security_definer: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod trigger_timing_display {
        use super::*;

        #[test]
        fn before_displays_uppercase() {
            assert_eq!(TriggerTiming::Before.to_string(), "BEFORE");
        }

        #[test]
        fn after_displays_uppercase() {
            assert_eq!(TriggerTiming::After.to_string(), "AFTER");
        }

        #[test]
        fn instead_of_displays_with_space() {
            assert_eq!(TriggerTiming::InsteadOf.to_string(), "INSTEAD OF");
        }
    }

    mod trigger_event_display {
        use super::*;

        #[test]
        fn insert_displays_uppercase() {
            assert_eq!(TriggerEvent::Insert.to_string(), "INSERT");
        }

        #[test]
        fn update_displays_uppercase() {
            assert_eq!(TriggerEvent::Update.to_string(), "UPDATE");
        }

        #[test]
        fn delete_displays_uppercase() {
            assert_eq!(TriggerEvent::Delete.to_string(), "DELETE");
        }

        #[test]
        fn truncate_displays_uppercase() {
            assert_eq!(TriggerEvent::Truncate.to_string(), "TRUNCATE");
        }
    }
}
