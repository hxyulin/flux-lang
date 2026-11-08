//! Backend worker capacity reserved by the crate graph scheduler.

use std::cell::Cell;
use std::num::NonZeroUsize;

thread_local! {
    static LIMIT: Cell<NonZeroUsize> = const { Cell::new(NonZeroUsize::MIN) };
}

/// Standalone and dependency jobs use one worker. The ready binary root may
/// reserve the graph budget after every reachable dependency has finished.
pub(crate) fn limit() -> NonZeroUsize {
    LIMIT.with(Cell::get)
}

pub(crate) fn with_limit<T>(jobs: usize, work: impl FnOnce() -> T) -> T {
    struct Restore(NonZeroUsize);
    impl Drop for Restore {
        fn drop(&mut self) {
            LIMIT.with(|limit| limit.set(self.0));
        }
    }
    let jobs = NonZeroUsize::new(jobs).unwrap_or(NonZeroUsize::MIN);
    let _restore = Restore(LIMIT.with(|limit| limit.replace(jobs)));
    work()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_restore_and_do_not_leak_into_other_threads() {
        assert_eq!(limit().get(), 1);
        with_limit(8, || {
            assert_eq!(limit().get(), 8);
            with_limit(2, || assert_eq!(limit().get(), 2));
            assert_eq!(limit().get(), 8);
            std::thread::scope(|scope| {
                scope.spawn(|| assert_eq!(limit().get(), 1));
            });
        });
        assert_eq!(limit().get(), 1);
    }
}
