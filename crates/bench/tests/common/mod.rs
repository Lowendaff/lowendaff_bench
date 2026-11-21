//! The progress log, captured in memory for the tests.

use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;

use bench::progress;
use tracing::subscriber::DefaultGuard;

/// One capture at a time in a test process. tracing records, for the whole process, which events
/// anyone listens to; a capture that starts on one thread while an event is first seen on
/// another can leave that event unheard when tests share a process, as under `cargo test`. A test
/// that logs without a capture is such another thread, so in a test binary that captures, every
/// test that logs captures too, even when it reads nothing.
///
/// Uses an [`AtomicBool`] rather than a [`Mutex`] so a failing assertion inside one test never
/// poisons subsequent tests or requires reaching inside a poisoned lock.
static ONE_AT_A_TIME: AtomicBool = AtomicBool::new(false);

struct Turn;

impl Turn {
    fn acquire() -> Self {
        while ONE_AT_A_TIME
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            thread::yield_now();
        }
        Self
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        ONE_AT_A_TIME.store(false, Ordering::Release);
    }
}

/// What the program logged on this thread.
#[derive(Clone, Default)]
pub struct Captured(Arc<Mutex<Vec<u8>>>);

/// Ends a capture when dropped: first the subscriber, then the turn of the next capture.
pub struct Capturing {
    _subscriber: DefaultGuard,
    _turn: Turn,
}

impl io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("no test thread panics while holding the log")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Captured {
    /// Logs this thread's events as the program does, but into memory, until the guard drops.
    pub fn start(verbose: bool) -> (Self, Capturing) {
        let turn = Turn::acquire();
        let captured = Self::default();
        let writer = captured.clone();
        let subscriber = progress::subscriber(move || writer.clone(), verbose);
        let capturing = Capturing {
            _subscriber: tracing::subscriber::set_default(subscriber),
            _turn: turn,
        };
        (captured, capturing)
    }

    /// The lines written, each without its `[hh:mm:ss] ` stamp, after checking the stamp.
    pub fn lines(&self) -> Vec<String> {
        let bytes = self
            .0
            .lock()
            .expect("no test thread panics while holding the log")
            .clone();
        let text = String::from_utf8(bytes).expect("the log is UTF-8");
        text.lines()
            .map(|line| {
                let (stamp, rest) = line
                    .split_once("] ")
                    .unwrap_or_else(|| panic!("a line starts with its time: {line:?}"));
                let digits = stamp.chars().filter(char::is_ascii_digit).count();
                assert!(
                    stamp.starts_with('[') && stamp.len() == 9 && digits == 6,
                    "the time is [hh:mm:ss]: {line:?}"
                );
                rest.to_owned()
            })
            .collect()
    }
}
