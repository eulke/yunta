//! Injected identity — the engine never mints a run id itself; whoever
//! drives it passes an [`IdSource`], the way time enters through
//! [`Clock`](crate::Clock). The binary passes [`SystemIdSource`], which
//! mints ULIDs; tests inject a sequential source and get reproducible ids.

use chrono::{DateTime, Utc};

use crate::RunId;

pub trait IdSource: Send + Sync {
    /// A run id no other run has, for a run born at `at`.
    fn mint_run_id(&self, at: DateTime<Utc>) -> RunId;
}

/// ULIDs: 26 Crockford base32 characters — the first ten encode the
/// millisecond of `at`, the rest are random — so ids sort by birth
/// instant, fit in a path and never collide across processes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemIdSource;

impl IdSource for SystemIdSource {
    fn mint_run_id(&self, at: DateTime<Utc>) -> RunId {
        RunId::from(ulid::Ulid::from_datetime(at.into()))
    }
}

/// Reproducible ids for tests: `<prefix>-1`, `<prefix>-2`, … in
/// minting order, whatever instant each is minted at.
#[cfg(any(test, feature = "testkit"))]
#[derive(Debug)]
pub struct SeqIdSource {
    prefix: &'static str,
    next: std::sync::atomic::AtomicU64,
}

#[cfg(any(test, feature = "testkit"))]
impl SeqIdSource {
    /// `prefix` is one path segment; minting panics otherwise.
    pub const fn new(prefix: &'static str) -> Self {
        SeqIdSource {
            prefix,
            next: std::sync::atomic::AtomicU64::new(1),
        }
    }
}

#[cfg(any(test, feature = "testkit"))]
impl IdSource for SeqIdSource {
    fn mint_run_id(&self, _at: DateTime<Utc>) -> RunId {
        let n = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        RunId::try_from(format!("{}-{n}", self.prefix))
            .expect("a path segment followed by `-<n>` is a run id")
    }
}
