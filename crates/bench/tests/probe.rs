//! Reachability means a completed TCP handshake with any one target.

use bench::probe;
use tokio::net::TcpListener;

#[tokio::test]
async fn reachable_needs_a_completed_handshake_with_any_target() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let open = listener.local_addr().unwrap().to_string();
    let closed = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .to_string();

    assert!(probe::reachable(&[open.as_str()]).await);
    assert!(!probe::reachable(&[closed.as_str()]).await);
    assert!(probe::reachable(&[closed.as_str(), open.as_str()]).await);
}
