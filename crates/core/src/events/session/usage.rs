//! The tokens a session spends, and the arithmetic every spend is summed
//! with.

use serde::{Deserialize, Serialize};

/// Tokens a session reported, or a sum of such reports: what every budget
/// and every spend the engine derives is counted in.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    /// The part of `input` the CLI read from its cache, when it says: a
    /// part of the input, never added to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<u64>,
}

impl TokenUsage {
    /// Input plus output — the single figure token cost is measured by,
    /// derived in one place so no two call sites can add it up differently.
    /// `cached` is a subset of `input`, already counted, never added on top.
    pub fn total(&self) -> u64 {
        self.input + self.output
    }

    /// Field by field, never below zero: usage that settles more than was
    /// in flight leaves nothing behind. `cached` stays unknown only while
    /// nothing reported it, as it does under `+`.
    pub fn saturating_sub(self, other: TokenUsage) -> TokenUsage {
        TokenUsage {
            input: self.input.saturating_sub(other.input),
            output: self.output.saturating_sub(other.output),
            cached: self
                .cached
                .map(|cached| cached.saturating_sub(other.cached.unwrap_or(0))),
        }
    }
}

/// Field by field; `cached` stays unknown only while nobody reported it.
impl std::ops::Add for TokenUsage {
    type Output = TokenUsage;

    fn add(self, other: TokenUsage) -> TokenUsage {
        TokenUsage {
            input: self.input + other.input,
            output: self.output + other.output,
            cached: match (self.cached, other.cached) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            },
        }
    }
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, other: TokenUsage) {
        *self = *self + other;
    }
}

impl std::iter::Sum for TokenUsage {
    fn sum<I: Iterator<Item = TokenUsage>>(iter: I) -> TokenUsage {
        iter.fold(TokenUsage::default(), |total, usage| total + usage)
    }
}
