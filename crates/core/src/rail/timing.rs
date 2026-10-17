//! How long a session's rail calls took, kept as a histogram that merges.
//!
//! A session keeps one histogram per moment and writes them at its close.
//! Histograms merge by adding their buckets, so the summary of several
//! sessions reads the percentiles of all of their calls at once.

use std::collections::BTreeMap;
use std::time::Duration;

use schemars::JsonSchema;
use serde::de::{Deserializer, Error as _};
use serde::{Deserialize, Serialize};

use super::RailEvent;
use crate::{ModelName, SessionId};

/// Three mantissa bits give eight buckets per octave, so a read stays within 12.5 percent.
const MANTISSA_BITS: u32 = 3;

/// Durations under this many microseconds each keep a bucket of their own.
const EXACT: u32 = 2 << MANTISSA_BITS;

/// Call durations counted per bucket, in microseconds on a log scale: a
/// bucket holds the durations that share their leading bits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Histogram {
    /// Every duration observed, in any bucket.
    pub count: u64,
    /// The count held by each bucket, keyed by its index.
    #[serde(deserialize_with = "keyed_by_index")]
    pub buckets: BTreeMap<u32, u64>,
}

/// Reads the buckets from their keys as text. A payload inside a tagged
/// enum hands its map keys over as strings, which a `u32` key cannot read.
fn keyed_by_index<'de, D>(de: D) -> Result<BTreeMap<u32, u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let keyed = BTreeMap::<String, u64>::deserialize(de)?;
    keyed
        .into_iter()
        .map(|(key, held)| -> Result<(u32, u64), D::Error> {
            let index = key.parse::<u32>().map_err(D::Error::custom)?;
            Ok((index, held))
        })
        .collect()
}

impl Histogram {
    /// Counts one call that took `took`.
    pub fn observe(&mut self, took: Duration) {
        let micros = u64::try_from(took.as_micros()).unwrap_or(u64::MAX);
        self.count += 1;
        *self.buckets.entry(bucket_of(micros)).or_default() += 1;
    }

    /// Adds the calls `other` holds to this histogram.
    pub fn merge(&mut self, other: &Histogram) {
        self.count += other.count;
        for (&bucket, &held) in &other.buckets {
            *self.buckets.entry(bucket).or_default() += held;
        }
    }

    /// Nearest rank, read as the upper bound of its bucket: never below the sample it names.
    pub fn percentile(&self, p: f64) -> Option<Duration> {
        if self.count == 0 {
            return None;
        }
        let rank = ((p * self.count as f64).ceil() as u64).clamp(1, self.count);
        let mut seen = 0;
        for (&bucket, &held) in &self.buckets {
            seen += held;
            if seen >= rank {
                return Some(Duration::from_micros(upper_of(bucket)));
            }
        }
        None
    }
}

/// The bucket a duration of `micros` falls in: its own bucket below `EXACT`,
/// and above that the octave's leading bits, `MANTISSA_BITS` of them.
fn bucket_of(micros: u64) -> u32 {
    if micros < u64::from(EXACT) {
        return micros as u32;
    }
    let octave = 63 - micros.leading_zeros();
    let shift = octave - MANTISSA_BITS;
    let leading = (micros >> shift) as u32;
    ((shift + 1) << MANTISSA_BITS) + (leading - (1 << MANTISSA_BITS))
}

/// The largest duration, in microseconds, that `bucket` holds.
fn upper_of(bucket: u32) -> u64 {
    if bucket < EXACT {
        return u64::from(bucket);
    }
    let shift = (bucket >> MANTISSA_BITS) - 1;
    let leading = (1 << MANTISSA_BITS) + (bucket & ((1 << MANTISSA_BITS) - 1));
    let next = u128::from(leading + 1) << shift;
    u64::try_from(next - 1).unwrap_or(u64::MAX)
}

/// What a session's rail calls took, written once when the session closes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RailSummaryPayload {
    pub session_id: SessionId,
    /// The model the rail heard first, when a hook named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
    /// One histogram per moment that was asked.
    pub moments: BTreeMap<RailEvent, Histogram>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Guard: pins the percentile read of a merged histogram.
    #[test]
    fn a_merged_histogram_reads_the_percentiles_of_all_its_samples() {
        let (mut a, mut b) = (Histogram::default(), Histogram::default());
        for ms in 1..=50 {
            a.observe(Duration::from_millis(ms));
        }
        for ms in 51..=100 {
            b.observe(Duration::from_millis(ms));
        }
        a.merge(&b);
        let p95 = a.percentile(0.95).unwrap();
        assert!(
            p95 >= Duration::from_millis(95) && p95 < Duration::from_millis(110),
            "{p95:?}"
        );
    }

    #[test]
    fn a_read_is_never_below_its_sample_and_stays_within_an_eighth_of_it() {
        for micros in [0, 1, 15, 16, 31, 32, 95_000, 3_600_000_000, u64::MAX] {
            let mut histogram = Histogram::default();
            histogram.observe(Duration::from_micros(micros));
            let read = histogram.percentile(1.0).unwrap().as_micros();
            let sample = u128::from(micros);
            assert!(
                read >= sample && read - sample <= sample / 8,
                "{sample} read as {read}"
            );
        }
    }

    #[test]
    fn an_empty_histogram_reads_no_percentile() {
        assert_eq!(Histogram::default().percentile(0.5), None);
    }
}
