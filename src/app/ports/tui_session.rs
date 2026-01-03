use color_eyre::eyre::Result;

/// Callback pattern ensures terminal is always restored, even on panic.
pub trait TuiSession {
    fn with_suspended<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce() -> R;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::rc::Rc;

    struct MockTuiSession {
        suspend_count: Rc<Cell<u32>>,
        resume_count: Rc<Cell<u32>>,
    }

    impl TuiSession for MockTuiSession {
        fn with_suspended<F, R>(&mut self, f: F) -> Result<R>
        where
            F: FnOnce() -> R,
        {
            self.suspend_count.set(self.suspend_count.get() + 1);

            struct ResumeGuard(Rc<Cell<u32>>);
            impl Drop for ResumeGuard {
                fn drop(&mut self) {
                    self.0.set(self.0.get() + 1);
                }
            }

            let _guard = ResumeGuard(Rc::clone(&self.resume_count));
            Ok(f())
        }
    }

    fn make_mock_session() -> (MockTuiSession, Rc<Cell<u32>>, Rc<Cell<u32>>) {
        let suspend_count = Rc::new(Cell::new(0));
        let resume_count = Rc::new(Cell::new(0));
        let session = MockTuiSession {
            suspend_count: Rc::clone(&suspend_count),
            resume_count: Rc::clone(&resume_count),
        };
        (session, suspend_count, resume_count)
    }

    mod with_suspended {
        use super::*;

        #[test]
        fn callback_returns_value() {
            let (mut session, suspend_count, resume_count) = make_mock_session();

            let result = session.with_suspended(|| 42).unwrap();

            assert_eq!(result, 42);
            assert_eq!(suspend_count.get(), 1);
            assert_eq!(resume_count.get(), 1);
        }

        #[test]
        fn panic_in_callback_still_resumes() {
            let (mut session, suspend_count, resume_count) = make_mock_session();

            let result = catch_unwind(AssertUnwindSafe(|| {
                let _ = session.with_suspended(|| panic!("test panic"));
            }));

            assert!(result.is_err());
            assert_eq!(suspend_count.get(), 1);
            assert_eq!(resume_count.get(), 1);
        }
    }
}
