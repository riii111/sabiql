/// A bounded scroll position. The owner supplies the current limit because it
/// depends on its content and viewport, both of which can change independently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollOffset(usize);

impl ScrollOffset {
    pub fn get(self) -> usize {
        self.0
    }
    pub fn set(&mut self, offset: usize) {
        self.0 = offset;
    }
    pub fn backward(&mut self, amount: usize) {
        self.0 = self.0.saturating_sub(amount);
    }
    pub fn forward(&mut self, amount: usize, max: usize) {
        self.0 = self.0.saturating_add(amount).min(max);
    }
    pub fn clamp(&mut self, max: usize) {
        self.0 = self.0.min(max);
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollOffset;

    #[test]
    fn scrolling_clamps_and_does_not_overflow() {
        let mut offset = ScrollOffset::default();
        offset.forward(usize::MAX, 10);
        assert_eq!(offset.get(), 10);
        offset.forward(usize::MAX, 10);
        assert_eq!(offset.get(), 10);
        offset.backward(usize::MAX);
        assert_eq!(offset.get(), 0);
        offset.set(9);
        offset.clamp(3);
        assert_eq!(offset.get(), 3);
    }
}
