//! A deterministic id source, so no assertion ever races a minted id.

use chrono::{DateTime, Utc};
use yunta_core::{IdSource, RunId};

/// Reproducible ids for tests: `<prefix>-1`, `<prefix>-2`, … in
/// minting order, whatever instant each is minted at.
#[derive(Debug)]
pub struct SeqIdSource {
    prefix: &'static str,
    next: std::sync::atomic::AtomicU64,
}

impl SeqIdSource {
    /// `prefix` is one path segment; minting panics otherwise.
    pub const fn new(prefix: &'static str) -> Self {
        SeqIdSource {
            prefix,
            next: std::sync::atomic::AtomicU64::new(1),
        }
    }
}

// A deterministic id source for tests: `<prefix>-<n>` is a run id by
// construction, so a fixture whose prefix breaks that aborts the test.
#[allow(clippy::expect_used)]
impl IdSource for SeqIdSource {
    fn mint_run_id(&self, _at: DateTime<Utc>) -> RunId {
        let n = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        RunId::try_from(format!("{}-{n}", self.prefix))
            .expect("a path segment followed by `-<n>` is a run id")
    }
}
