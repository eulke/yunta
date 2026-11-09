//! What a count of tokens reads as in a line that says what was spent, and
//! the same figures as a JSON document.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use surco::figures::{duration, Tokens};
use yunta_core::events::{Spend, TokenUsage};

/// A count read as its breakdown: `1.86M in / 20.2k out (9k cached)`. A cache
/// that served nothing is no part of it.
pub struct Breakdown<'a>(pub &'a TokenUsage);

impl fmt::Display for Breakdown<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} in / {} out",
            Tokens(self.0.input).figure(),
            Tokens(self.0.output).figure()
        )?;
        self.0
            .cached
            .filter(|cached| *cached > 0)
            .map_or(Ok(()), |c| write!(f, " ({} cached)", Tokens(c).figure()))
    }
}

/// The figures a JSON document carries for a spend. `cached` is left out
/// when nothing reported it, as it is in the JSON of [`TokenUsage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SpendJson {
    pub input: u64,
    pub output: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<u64>,
    pub total: u64,
    pub in_flight: u64,
}

impl SpendJson {
    /// The spend so far, as every surface reads it, with the usage still
    /// in flight counted on its own.
    pub fn of(spend: &Spend) -> SpendJson {
        let so_far = spend.so_far();
        SpendJson {
            input: so_far.input,
            output: so_far.output,
            cached: so_far.cached,
            total: so_far.total(),
            in_flight: spend.in_flight.total(),
        }
    }
}

/// What a workflow's past runs usually spent, as one line: how many there
/// are, the median and p90 of their tokens, and their median wall-clock. The
/// history, the catalog and a run's page say it here, so none phrases the
/// same numbers differently.
pub fn estimation(
    estimation: &yunta_replay::history::PriorEstimation,
    glyphs: surco::glyphs::Glyphs,
) -> String {
    let sep = glyphs.sep();
    let wall_clock = match estimation.wall_clock_secs {
        Some(p) => duration(Duration::from_secs_f64(p.median)),
        None => "n/a".to_string(),
    };
    format!(
        "{} {sep} median {}, p90 {} {sep} median wall-clock {}",
        surco::text::counted(estimation.sample_count, "past run"),
        Tokens::rounded(estimation.tokens.median),
        Tokens::rounded(estimation.tokens.p90).figure(),
        wall_clock,
    )
}

/// What the run's spend costs at its model's price: one blended line when
/// several models are priced, and none when no price is known.
pub fn cost(
    tokens: u64,
    pricing: Option<&BTreeMap<String, yunta_core::PricingEntry>>,
) -> Option<String> {
    // A currency estimate needs a *model* to price against; with more
    // than one model priced and no per-node attribution surfaced here,
    // showing one blended-average line beats showing none — never
    // silently picking the first entry a HashMap happens to iterate.
    let pricing = pricing?;
    if pricing.is_empty() {
        return None;
    }
    let avg_per_1k: f64 = pricing
        .values()
        .map(|entry| entry.cost_per_1k_tokens)
        .sum::<f64>()
        / pricing.len() as f64;
    let estimate = (tokens as f64 / 1000.0) * avg_per_1k;
    Some(format!(
        "~{estimate:.2} (avg of {}, never authoritative)",
        surco::text::counted(pricing.len(), "priced model")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakdown_names_the_cache_only_when_something_reported_it() {
        let plain = TokenUsage {
            input: 1_860_000,
            output: 20_200,
            cached: None,
        };
        assert_eq!(Breakdown(&plain).to_string(), "1.86M in / 20.2k out");
        let cached = TokenUsage {
            cached: Some(9_000),
            ..plain
        };
        assert_eq!(
            Breakdown(&cached).to_string(),
            "1.86M in / 20.2k out (9k cached)"
        );
    }

    /// A cache that served nothing is no part of the breakdown: a count that
    /// reports zero cached tokens names no cache.
    #[test]
    fn breakdown_names_no_cache_that_served_nothing() {
        let served_nothing = TokenUsage {
            input: 1_860_000,
            output: 20_200,
            cached: Some(0),
        };
        assert_eq!(
            Breakdown(&served_nothing).to_string(),
            "1.86M in / 20.2k out"
        );
    }

    #[test]
    fn spend_json_adds_the_usage_in_flight_to_the_settled_usage() {
        let spend = Spend {
            settled: TokenUsage {
                input: 30,
                output: 12,
                cached: Some(10),
            },
            in_flight: TokenUsage {
                input: 5,
                output: 3,
                cached: None,
            },
        };
        assert_eq!(
            SpendJson::of(&spend),
            SpendJson {
                input: 35,
                output: 15,
                cached: Some(10),
                total: 50,
                in_flight: 8,
            }
        );
    }
}
