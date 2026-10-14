//! Cadence of a poll over state another process owns. The condition and the
//! deadline stay at the call site; this type only spaces the looks.

/// How long a poll waits between two looks at the state it watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace(std::time::Duration);

impl Pace {
    /// A poll that looks once per `every`.
    pub const fn every(every: std::time::Duration) -> Self {
        Self(every)
    }

    /// Waits one look's interval.
    pub async fn beat(self) {
        tokio::time::sleep(self.0).await;
    }
}
