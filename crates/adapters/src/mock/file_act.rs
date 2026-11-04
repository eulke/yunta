//! What a scripted call does to its file.

use serde::Deserialize;

/// What a scripted call does to its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAct {
    Read,
    Edit,
    Write,
}
