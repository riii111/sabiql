use crate::cmd::effect::Effect;

#[derive(Debug, Clone)]
pub enum DispatchResult {
    Pass,
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

    pub fn or_else<F>(self, f: F) -> Self
    where
        F: FnOnce() -> Self,
    {
        match self {
            Self::Pass => f(),
            Self::Handled(_) => self,
        }
    }

    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    pub fn is_handled(&self) -> bool {
        matches!(self, Self::Handled(_))
    }

    pub fn is_handled_and<F>(&self, f: F) -> bool
    where
        F: FnOnce(&Vec<Effect>) -> bool,
    {
        match self {
            Self::Handled(effects) => f(effects),
            Self::Pass => false,
        }
    }

    #[cfg(test)]
    pub fn unwrap(self) -> Vec<Effect> {
        self.into_effects().unwrap()
    }

    #[cfg(test)]
    pub fn expect(self, msg: &str) -> Vec<Effect> {
        self.into_effects().expect(msg)
    }
}
