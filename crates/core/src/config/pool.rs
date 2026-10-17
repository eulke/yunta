//! `storage.pool`: what the project's checkouts may keep of the disk. A
//! build is the weight of a checkout — gigabytes where its source is
//! megabytes — so the pool keeps its builds within a budget, and always
//! leaves part of the disk free for everything else.

use serde::{Deserialize, Serialize};

/// `storage.pool` — both optional: what is not said falls back to what
/// the disk and the builds the pool measured say.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PoolConfig {
    /// What the builds of the project's checkouts may take together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<DiskShare>,
    /// What of the disk the pool always leaves free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_free: Option<DiskShare>,
}

/// An amount of a disk: bytes — `80G`, `500M`, `1.5T`, in decimal units —
/// or a share of the whole file system, `20%`.
#[derive(Debug, Clone, Copy, PartialEq, schemars::JsonSchema)]
#[schemars(with = "String")]
pub enum DiskShare {
    Bytes(u64),
    Percent(u8),
}

impl DiskShare {
    /// This amount of a file system holding `total` bytes.
    pub fn of(self, total: u64) -> u64 {
        match self {
            DiskShare::Bytes(bytes) => bytes,
            DiskShare::Percent(percent) => total / 100 * u64::from(percent),
        }
    }
}

impl std::str::FromStr for DiskShare {
    type Err = String;

    fn from_str(said: &str) -> Result<Self, Self::Err> {
        let said = said.trim();
        let refused = || {
            format!("`{said}` is no amount of a disk: write bytes as `80G`, `500M`, `1.5T`, or a share as `20%`")
        };
        if let Some(percent) = said.strip_suffix('%') {
            return match percent.trim().parse::<u8>() {
                Ok(percent) if percent <= 100 => Ok(DiskShare::Percent(percent)),
                _ => Err(refused()),
            };
        }
        let number = said.trim_end_matches(['B', 'b']);
        let scales = [('k', 1e3), ('m', 1e6), ('g', 1e9), ('t', 1e12)];
        let (number, scale) = scales
            .iter()
            .find_map(|(unit, scale)| {
                number
                    .strip_suffix([*unit, unit.to_ascii_uppercase()])
                    .map(|number| (number, *scale))
            })
            .unwrap_or((number, 1.0));
        match number.trim().parse::<f64>() {
            Ok(amount) if amount >= 0.0 && amount.is_finite() => {
                Ok(DiskShare::Bytes((amount * scale).round() as u64))
            }
            _ => Err(refused()),
        }
    }
}

impl std::fmt::Display for DiskShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskShare::Bytes(bytes) => write!(f, "{}", crate::units::Bytes(*bytes)),
            DiskShare::Percent(percent) => write!(f, "{percent}%"),
        }
    }
}

impl Serialize for DiskShare {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            DiskShare::Bytes(bytes) => serializer.serialize_str(&bytes.to_string()),
            DiskShare::Percent(percent) => serializer.serialize_str(&format!("{percent}%")),
        }
    }
}

impl<'de> Deserialize<'de> for DiskShare {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let said = String::deserialize(deserializer)?;
        said.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::DiskShare;

    #[test]
    fn an_amount_of_a_disk_is_bytes_or_a_share() {
        let read = |said: &str| said.parse::<DiskShare>();
        assert_eq!(read("80G"), Ok(DiskShare::Bytes(80_000_000_000)));
        assert_eq!(read("1.5TB"), Ok(DiskShare::Bytes(1_500_000_000_000)));
        assert_eq!(read("20%"), Ok(DiskShare::Percent(20)));
        assert!(read("120%").is_err() && read("lots").is_err());
        assert_eq!(DiskShare::Percent(20).of(1_000), 200);
    }

    #[test]
    fn an_amount_written_back_reads_the_same() {
        for share in [DiskShare::Bytes(80_000_000_000), DiskShare::Percent(5)] {
            let written = serde_json::to_string(&share).unwrap();
            assert_eq!(serde_json::from_str::<DiskShare>(&written).unwrap(), share);
        }
    }
}
