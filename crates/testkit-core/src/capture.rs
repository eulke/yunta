//! A writer a test reads back.

use std::io::Write;
use std::sync::{Arc, Mutex};

/// A `Write` that keeps what is written to it instead of printing it —
/// what a surface writing to a `Box<dyn Write + Send>` is handed when
/// the test wants to assert on the lines a person would have seen.
///
/// Cloning shares the bytes, so the copy a test keeps reads what the
/// copy it handed over wrote.
#[derive(Clone, Debug, Default)]
pub struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    /// Everything written so far, as text (lossy on non-UTF-8).
    pub fn text(&self) -> String {
        self.0
            .lock()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    }
}

impl Write for Captured {
    /// A poisoned lock means another thread panicked mid-write; the
    /// bytes are dropped rather than failing the write, so the assertion
    /// a test is about to make reports what it saw instead of an error
    /// about the lock.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut bytes) = self.0.lock() {
            bytes.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
