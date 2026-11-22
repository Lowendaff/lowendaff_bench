//! Unit tests for `bench_api::run_bounded`.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use bench_api::CancellationToken;

#[tokio::test]
async fn run_bounded_preserves_order_caps_concurrency_and_stops_spawning_on_cancel() {
    let cancel = CancellationToken::new();
    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let tasks = (0..25).map(|idx| {
        let in_flight = Arc::clone(&in_flight);
        let peak = Arc::clone(&peak);
        async move {
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            tokio::task::yield_now().await;
            in_flight.fetch_sub(1, Ordering::SeqCst);
            idx
        }
    });
    let ordered = bench_api::run_bounded(4, &cancel, tasks).await;
    assert_eq!(ordered, (0..25).collect::<Vec<_>>());
    assert_eq!(peak.load(Ordering::SeqCst), 4);

    // Cancellation mid-batch must drop active futures and never poll remaining queued tasks.
    let cancel2 = CancellationToken::new();
    let polled = Arc::new(AtomicUsize::new(0));
    let tasks2 = (0..30).map(|idx| {
        let polled = Arc::clone(&polled);
        let cancel2 = cancel2.clone();
        async move {
            polled.fetch_add(1, Ordering::SeqCst);
            if idx == 1 {
                cancel2.cancel();
            }
            std::future::pending::<usize>().await
        }
    });
    let cancelled_out = bench_api::run_bounded(3, &cancel2, tasks2).await;
    assert!(cancelled_out.is_empty());
    assert!(
        polled.load(Ordering::SeqCst) <= 3,
        "queued tasks past the active window must never be polled after cancel"
    );
}
