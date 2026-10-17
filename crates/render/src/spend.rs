//! What a count of tokens reads as in a line that says what was spent, and
//! the same figures as a JSON document.

use std::fmt;

use yunta_core::events::{Spend, TokenUsage};
use yunta_core::units::Tokens;

/// A count read as its breakdown: `1.86M in / 20.2k out (9k cached)`.
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
