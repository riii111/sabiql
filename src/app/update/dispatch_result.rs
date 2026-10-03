use crate::cmd::effect::Effect;

/// Outcome of dispatching an action to a reducer.
///
/// `Pass` keeps the dispatcher chain moving. `Handled` stops the chain and
/// returns the effects produced by the reducer, if any.
#[derive(Debug, Clone)]
pub enum DispatchResult {
    /// The reducer did not handle the action.
    Pass,
    /// The reducer handled the action and produced zero or more effects.
    Handled(Vec<Effect>),
}

impl DispatchResult {
    pub fn pass() -> Self {
        Self::Pass
    }

    pub fn handled() -> Self {
        Self::Handled(vec![])
    }

    pub fn handled_with(effects: Vec<Effect>) -> Self {
        Self::Handled(effects)
    }

    pub fn into_effects(self) -> Option<Vec<Effect>> {
        match self {
            Self::Pass => None,
            Self::Handled(effects) => Some(effects),
        }
    }

    #[must_use]
    pub fn or_else<F>(self, f: F) -> Self
    where
        F: FnOnce() -> Self,
    {
        match self {
            Self::Pass => f(),
            Self::Handled(_) => self,
        }
    }
}

#[cfg(test)]
mod test_support {
    use super::{DispatchResult, Effect};

    impl DispatchResult {
        pub fn is_pass(&self) -> bool {
            matches!(self, Self::Pass)
        }

        pub fn is_handled(&self) -> bool {
            matches!(self, Self::Handled(_))
        }

        pub fn unwrap(self) -> Vec<Effect> {
            self.into_effects().unwrap()
        }

        pub fn expect(self, msg: &str) -> Vec<Effect> {
            self.into_effects().expect(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn or_else_keeps_handled_result() {
        let result = DispatchResult::handled_with(vec![Effect::Render]);

        let chained = result.or_else(DispatchResult::handled);

        assert!(matches!(
            chained.into_effects().as_deref(),
            Some([Effect::Render])
        ));
    }

    #[test]
    fn or_else_dispatches_next_on_pass() {
        let result = DispatchResult::pass();

        let chained = result.or_else(|| DispatchResult::handled_with(vec![Effect::Render]));

        assert!(matches!(
            chained.into_effects().as_deref(),
            Some([Effect::Render])
        ));
    }
}
