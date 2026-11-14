//! Trying again: which failures are tried again, how many times, the wait between attempts, and
//! a cancellation during the wait.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_tool::retried::Retried;
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// Worth another try.
    Again,
    /// Not worth another try.
    Final,
}

fn worth_another_try(why: &Why) -> bool {
    *why == Why::Again
}

/// Runs the attempts of `script`, in order, and also returns how many ran.
async fn attempts(
    limit: usize,
    wait: Duration,
    cancel: &CancellationToken,
    script: Vec<Result<u32, Why>>,
) -> (Retried<u32, Why>, usize) {
    let script = RefCell::new(VecDeque::from(script));
    let ran = RefCell::new(0);
    let retried = Retried::run(limit, wait, cancel, worth_another_try, || {
        *ran.borrow_mut() += 1;
        let next = script
            .borrow_mut()
            .pop_front()
            .expect("every attempt is scripted");
        async move { next }
    })
    .await;
    (retried, ran.into_inner())
}

#[tokio::test]
async fn the_first_success_ends_the_attempts() {
    let cancel = CancellationToken::new();
    let script = vec![Err(Why::Again), Ok(7), Ok(8)];
    let (retried, ran) = attempts(4, Duration::ZERO, &cancel, script).await;
    assert_eq!(
        retried,
        Retried {
            failures: vec![Why::Again],
            last: Ok(7),
        }
    );
    assert_eq!(ran, 2);
}

#[tokio::test]
async fn a_failure_not_worth_another_try_ends_the_attempts() {
    let cancel = CancellationToken::new();
    let script = vec![Err(Why::Again), Err(Why::Final), Ok(8)];
    let (retried, ran) = attempts(4, Duration::ZERO, &cancel, script).await;
    assert_eq!(
        retried,
        Retried {
            failures: vec![Why::Again],
            last: Err(Why::Final),
        }
    );
    assert_eq!(ran, 2);
}

#[tokio::test]
async fn the_attempts_stop_at_the_limit() {
    let cancel = CancellationToken::new();
    let script = vec![Err(Why::Again); 5];
    let (retried, ran) = attempts(4, Duration::ZERO, &cancel, script).await;
    assert_eq!(
        retried,
        Retried {
            failures: vec![Why::Again; 3],
            last: Err(Why::Again),
        }
    );
    assert_eq!(ran, 4);
}

#[tokio::test]
async fn one_attempt_runs_whatever_the_limit() {
    let cancel = CancellationToken::new();
    for limit in [0, 1] {
        let script = vec![Err(Why::Again)];
        let (retried, ran) = attempts(limit, Duration::ZERO, &cancel, script).await;
        assert_eq!(retried.last, Err(Why::Again), "limit {limit}");
        assert_eq!(ran, 1, "limit {limit}");
    }
}

#[tokio::test(start_paused = true)]
async fn each_attempt_after_a_failure_waits_first() {
    let cancel = CancellationToken::new();
    let started = Instant::now();
    let script = vec![Err(Why::Again), Err(Why::Again), Ok(7)];
    let (retried, _) = attempts(4, Duration::from_secs(30), &cancel, script).await;
    assert_eq!(retried.last, Ok(7));
    assert_eq!(started.elapsed(), Duration::from_secs(60), "two waits");
}

#[tokio::test(start_paused = true)]
async fn a_cancellation_during_the_wait_ends_the_attempts_at_once() {
    let cancel = CancellationToken::new();
    let canceller = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        canceller.cancel();
    });
    let started = Instant::now();
    let script = vec![Err(Why::Again), Ok(7)];
    let (retried, ran) = attempts(4, Duration::from_secs(30), &cancel, script).await;
    assert_eq!(
        retried,
        Retried {
            failures: Vec::new(),
            last: Err(Why::Again),
        },
        "the last failure stands"
    );
    assert_eq!(ran, 1);
    assert_eq!(started.elapsed(), Duration::from_secs(10));
}

/// With the clock paused, a wait of zero is over as soon as it starts, so the wait and the
/// cancellation are ready together; the cancellation must win every time, not half of them.
#[tokio::test(start_paused = true)]
async fn after_a_cancellation_no_attempt_follows() {
    for _ in 0..20 {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let script = vec![Err(Why::Again), Ok(7)];
        let (retried, ran) = attempts(4, Duration::ZERO, &cancel, script).await;
        assert_eq!(retried.last, Err(Why::Again));
        assert_eq!(ran, 1);
    }
}
