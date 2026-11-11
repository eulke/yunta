//! See [`crate`]. Cadence of a poll over state another process owns. The condition and the
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

    /// Waits until one look's interval has passed since the look at `last`: however
    /// often the wait is begun again, as a poll that hands back other news between its
    /// looks begins it, the next look comes on time, and one overdue comes at once.
    pub async fn beat_after(self, last: tokio::time::Instant) {
        tokio::time::sleep_until(last + self.0).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Pace;

    /// The interval the test's looks keep: long beside the instant an overdue wait takes.
    const EVERY: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn a_look_overdue_comes_at_once_however_often_its_wait_begins_again() {
        let pace = Pace::every(EVERY);
        let last = tokio::time::Instant::now() - EVERY;
        let started = tokio::time::Instant::now();
        for _ in 0..3 {
            pace.beat_after(last).await;
        }
        assert!(started.elapsed() < EVERY, "{:?}", started.elapsed());
    }
}
