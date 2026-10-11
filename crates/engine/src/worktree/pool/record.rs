//! The only writer of the pool's records: each checkout's hold, its use,
//! its owner and its build. A record is written while the caller holds the
//! checkout it describes, so the hold orders the writers, across processes
//! too.

use std::io;
use std::path::Path;

use serde::Serialize;
use yunta_core::durable::{self, Durability};

/// Writes `value` as JSON at `path`, whole: a reader meets the record as it
/// was or as it is now, never half of either. Called only under the hold of
/// the checkout the record describes.
pub(super) fn write<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let json = serde_json::to_vec(value).map_err(io::Error::other)?;
    durable::replace(path, &json, Durability::Atomic)
}
