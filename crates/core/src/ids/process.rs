//! The process id: a positive number the kernel hands out, checked once
//! at the edge where it is read.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::InvalidId;

/// A process id: a positive number, as the kernel hands them out. Zero
/// and negatives — what a signal call or a lock file could otherwise
/// carry by mistake — are unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pid(u32);

const PID_RULE: &str = "a positive number that fits a signed 32-bit integer";

impl Pid {
    /// This process's own id. The kernel never runs a user process as
    /// pid 0, so no check is needed here.
    pub fn current() -> Pid {
        Pid(std::process::id())
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// The id as the kernel's own type; the constructors guarantee it
    /// fits.
    pub const fn as_i32(self) -> i32 {
        self.0 as i32
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<u32> for Pid {
    type Error = InvalidId;

    fn try_from(value: u32) -> Result<Self, InvalidId> {
        if value == 0 || i32::try_from(value).is_err() {
            return Err(InvalidId {
                what: "process id",
                value: value.to_string(),
                rule: PID_RULE,
            });
        }
        Ok(Pid(value))
    }
}

impl TryFrom<i32> for Pid {
    type Error = InvalidId;

    fn try_from(value: i32) -> Result<Self, InvalidId> {
        u32::try_from(value)
            .ok()
            .filter(|raw| *raw > 0)
            .map(Pid)
            .ok_or_else(|| InvalidId {
                what: "process id",
                value: value.to_string(),
                rule: PID_RULE,
            })
    }
}

impl Serialize for Pid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for Pid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u32::deserialize(deserializer)?;
        Pid::try_from(value).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Pid {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Pid".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "integer",
            "minimum": 1,
            "maximum": i32::MAX,
            "description": format!("A process id: {}", PID_RULE),
        })
    }
}
