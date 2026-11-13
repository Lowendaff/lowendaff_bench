//! Trying a measurement again after it fails, as the test items do: up to a number of attempts,
//! only after the failures worth another try, with a wait between attempts that a cancellation
//! cuts short.

use std::future::Future;
use std::time::Duration;

use bench_api::CancellationToken;

/// The attempts at one measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retried<T, E> {
    /// The errors of the attempts before the last one, oldest first; each was worth another try.
    pub failures: Vec<E>,
    /// How the last attempt ended.
    pub last: Result<T, E>,
}

impl<T, E> Retried<T, E> {
    /// Runs `attempt` until it succeeds, fails in a way that `again` does not try again, or has
    /// run `limit` times, and at least once; between attempts it waits for `wait`. A cancellation
    /// ends the attempts at once, and the last failure stands.
    pub async fn run<F, Fut>(
        limit: usize,
        wait: Duration,
        cancel: &CancellationToken,
        again: fn(&E) -> bool,
        mut attempt: F,
    ) -> Self
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let mut failures = Vec::new();
        loop {
            let error = match attempt().await {
                Err(error) if again(&error) && failures.len() + 1 < limit => error,
                last => return Self { failures, last },
            };
            tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    return Self {
                        failures,
                        last: Err(error),
                    };
                }
                () = tokio::time::sleep(wait) => failures.push(error),
            }
        }
    }
}
