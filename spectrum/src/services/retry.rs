use crate::config::store::Error;
use futures_retry::RetryPolicy;
use log::trace;
use std::fmt;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
use std::time::Duration;

// Try `attempts` times, delaying for duration `delay` between each.
pub fn error_policy<T: fmt::Debug>(
    delay: Duration,
    attempts: usize,
) -> impl FnMut(T) -> RetryPolicy<Error> {
    assert!(attempts > 0, "Zero attempts makes no sense.");
    let counter = Mutex::new(AtomicUsize::new(attempts));
    move |err: T| {
        trace!("Error: {:?}", err);
        let count = counter.lock().unwrap();
        if count.load(Ordering::Relaxed) <= 1 {
            let msg = format!("Did not succeed in {} attempts.", attempts);
            RetryPolicy::ForwardError(Error::new(&msg))
        } else {
            count.fetch_sub(1, Ordering::Relaxed);
            RetryPolicy::WaitRetry(delay)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{
        future::{err, ok},
        TryFuture,
    };
    use futures_retry::{FutureFactory, FutureRetry};
    use proptest::prelude::*;
    use std::iter::{once, repeat};
    use tokio::runtime::Runtime;

    const NO_DELAY: Duration = Duration::from_millis(0);

    // Test helper for retry policy -- inspired by futures_retry tests.
    struct FutureIterator<F>(F);

    impl<I, F> FutureFactory for FutureIterator<I>
    where
        I: Unpin + Iterator<Item = F>,
        F: TryFuture,
    {
        type FutureItem = F;

        fn new(&mut self) -> Self::FutureItem {
            self.0.next().expect("No more futures!")
        }
    }

    #[should_panic]
    #[test]
    fn test_error_policy_zero_attempts() {
        let _ = error_policy::<()>(NO_DELAY, 0);
    }

    proptest! {
        #[test]
        fn test_error_policy_many_attempts_success(attempts in 1usize..10usize) {
            let runtime = Runtime::new().unwrap();
            runtime.block_on(async {
                let results = repeat(err(()))
                    .take(attempts - 1)
                    .chain(once(ok(())));
                FutureRetry::new(FutureIterator(results), error_policy(NO_DELAY, attempts))
                    .await
                    .expect("Had enough attempts to succeed!");
            })
        }

        #[test]
        fn test_error_policy_many_attempts_failure(attempts in 1usize..10usize) {
            let runtime = Runtime::new().unwrap();
            runtime.block_on(async {
                let results = repeat(err(()))
                    .take(attempts)
                    .chain(once(ok(())));
                FutureRetry::new(FutureIterator(results), error_policy(NO_DELAY, attempts))
                    .await
                    .expect_err("Didn't have enough attempts to succeed!");
            })
        }
    }
}
