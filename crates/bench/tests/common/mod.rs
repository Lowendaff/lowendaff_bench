//! The progress log, captured in memory for the tests.

use std::io;
use std::sync::Arc;
use std::sync::Mutex;

use bench::progress;
use tracing::subscriber::DefaultGuard;

/// What the program logged on this thread.
#[derive(Clone, Default)]
pub struct Captured(Arc<Mutex<Vec<u8>>>);

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
    pub fn start(verbose: bool) -> (Self, DefaultGuard) {
        let captured = Self::default();
        let writer = captured.clone();
        let subscriber = progress::subscriber(move || writer.clone(), verbose);
        (captured, tracing::subscriber::set_default(subscriber))
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
