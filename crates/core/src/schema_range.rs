//! `yunta_schema:` — the comparator range a workflow or a pack states
//! the binary's schema major must satisfy.
//!
//! A hand-rolled range rather than a `semver` dependency: what is
//! compared is one whole number, the schema major, and the syntax a
//! range may use is the four comparators below and equality. The day
//! something needs to compare two full versions, the dependency comes
//! in then.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A range that parsed: every comparator in it is one this evaluates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaRange {
    text: String,
    comparators: Vec<Comparator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Comparator {
    op: Operator,
    version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    AtLeast,
    AtMost,
    Above,
    Below,
    Exactly,
}

/// A range the parser cannot read. An empty range does not parse
/// either — a declared requirement that constrains nothing is a typo,
/// not a wildcard.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchemaRangeError {
    #[error("the range is empty")]
    Empty,
    #[error("comparator `{comparator}` has no version number")]
    NoVersion { comparator: String },
    #[error("`{text}` is not a whole schema version")]
    NotAVersion { text: String },
    #[error("unknown comparator `{op}`")]
    UnknownOperator { op: String },
}

impl SchemaRange {
    /// Whether `binary` satisfies every comparator in the range.
    pub fn holds_for(&self, binary: u32) -> bool {
        self.comparators
            .iter()
            .all(|comparator| comparator.holds_for(binary))
    }

    /// The range as authored — what a diagnostic prints.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The range one whole schema major satisfies and no other — what a
    /// run records when the workflow declared none.
    pub fn exactly(version: u32) -> Self {
        SchemaRange {
            text: format!("={version}"),
            comparators: vec![Comparator {
                op: Operator::Exactly,
                version,
            }],
        }
    }
}

impl Comparator {
    fn holds_for(self, binary: u32) -> bool {
        match self.op {
            Operator::AtLeast => binary >= self.version,
            Operator::AtMost => binary <= self.version,
            Operator::Above => binary > self.version,
            Operator::Below => binary < self.version,
            Operator::Exactly => binary == self.version,
        }
    }
}

impl FromStr for SchemaRange {
    type Err = SchemaRangeError;

    fn from_str(text: &str) -> Result<Self, SchemaRangeError> {
        let comparators: Vec<Comparator> = text
            .split_whitespace()
            .map(Comparator::from_str)
            .collect::<Result<_, _>>()?;
        if comparators.is_empty() {
            return Err(SchemaRangeError::Empty);
        }
        Ok(SchemaRange {
            text: text.to_string(),
            comparators,
        })
    }
}

impl FromStr for Comparator {
    type Err = SchemaRangeError;

    fn from_str(comparator: &str) -> Result<Self, SchemaRangeError> {
        let (op, number) = comparator
            .find(|c: char| c.is_ascii_digit())
            .map(|i| comparator.split_at(i))
            .ok_or_else(|| SchemaRangeError::NoVersion {
                comparator: comparator.to_string(),
            })?;
        let version: u32 = number.parse().map_err(|_| SchemaRangeError::NotAVersion {
            text: number.to_string(),
        })?;
        let op = match op {
            ">=" => Operator::AtLeast,
            "<=" => Operator::AtMost,
            ">" => Operator::Above,
            "<" => Operator::Below,
            "=" | "==" | "" => Operator::Exactly,
            other => {
                return Err(SchemaRangeError::UnknownOperator {
                    op: other.to_string(),
                })
            }
        };
        Ok(Comparator { op, version })
    }
}

impl TryFrom<String> for SchemaRange {
    type Error = SchemaRangeError;

    fn try_from(text: String) -> Result<Self, SchemaRangeError> {
        text.parse()
    }
}

impl fmt::Display for SchemaRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl Serialize for SchemaRange {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for SchemaRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for SchemaRange {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SchemaRange".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "description": "a schema range: whitespace-separated comparators (`>=`, `<=`, `>`, `<`, `=`) over whole schema majors, e.g. `>=1 <2`",
        })
    }
}

/// Test convenience: a literal that does not parse panics with the
/// reason. Production code parses instead.
#[cfg(any(test, feature = "testkit"))]
// A literal that does not parse is a test-authoring mistake, meant to
// abort the test loudly — the one place a panic is the right answer.
#[allow(clippy::panic)]
impl From<&str> for SchemaRange {
    fn from(text: &str) -> Self {
        match text.parse() {
            Ok(range) => range,
            Err(error) => panic!("{error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_holds_only_when_every_comparator_does() {
        let range: SchemaRange = ">=1 <2".parse().unwrap();
        assert!(range.holds_for(1));
        assert!(!range.holds_for(0));
        assert!(!range.holds_for(2));
    }

    #[test]
    fn a_bare_number_is_an_equality() {
        let range: SchemaRange = "1".parse().unwrap();
        assert!(range.holds_for(1));
        assert!(!range.holds_for(2));
    }

    #[test]
    fn a_range_that_constrains_nothing_does_not_parse() {
        assert_eq!("   ".parse::<SchemaRange>(), Err(SchemaRangeError::Empty));
    }

    #[test]
    fn an_unknown_comparator_names_itself() {
        assert_eq!(
            "~>1".parse::<SchemaRange>(),
            Err(SchemaRangeError::UnknownOperator {
                op: "~>".to_string()
            })
        );
    }
}
