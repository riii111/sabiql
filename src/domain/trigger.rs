use std::fmt;
use std::str::FromStr;

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

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ParseTriggerTimingError {
    #[error("invalid trigger timing: {input}")]
    Invalid { input: String },
}

impl FromStr for TriggerTiming {
    type Err = ParseTriggerTimingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            input if input.eq_ignore_ascii_case("BEFORE") => Ok(Self::Before),
            input if input.eq_ignore_ascii_case("AFTER") => Ok(Self::After),
            input if input.eq_ignore_ascii_case("INSTEAD OF") => Ok(Self::InsteadOf),
            _ => Err(ParseTriggerTimingError::Invalid {
                input: s.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ParseTriggerEventError {
    #[error("invalid trigger event: {input}")]
    Invalid { input: String },
}

impl FromStr for TriggerEvent {
    type Err = ParseTriggerEventError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            input if input.eq_ignore_ascii_case("INSERT") => Ok(Self::Insert),
            input if input.eq_ignore_ascii_case("UPDATE") => Ok(Self::Update),
            input if input.eq_ignore_ascii_case("DELETE") => Ok(Self::Delete),
            input if input.eq_ignore_ascii_case("TRUNCATE") => Ok(Self::Truncate),
            _ => Err(ParseTriggerEventError::Invalid {
                input: s.to_string(),
            }),
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
    use rstest::rstest;

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

        #[rstest]
        #[case(TriggerTiming::Before)]
        #[case(TriggerTiming::After)]
        #[case(TriggerTiming::InsteadOf)]
        fn display_round_trips(#[case] timing: TriggerTiming) {
            assert_eq!(timing.to_string().parse::<TriggerTiming>().unwrap(), timing);
        }

        #[test]
        fn from_str_rejects_unknown_timing() {
            assert!(matches!(
                "unknown".parse::<TriggerTiming>(),
                Err(ParseTriggerTimingError::Invalid { .. })
            ));
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

        #[rstest]
        #[case(TriggerEvent::Insert)]
        #[case(TriggerEvent::Update)]
        #[case(TriggerEvent::Delete)]
        #[case(TriggerEvent::Truncate)]
        fn display_round_trips(#[case] event: TriggerEvent) {
            assert_eq!(event.to_string().parse::<TriggerEvent>().unwrap(), event);
        }

        #[test]
        fn from_str_rejects_unknown_event() {
            assert!(matches!(
                "merge".parse::<TriggerEvent>(),
                Err(ParseTriggerEventError::Invalid { .. })
            ));
        }
    }
}
