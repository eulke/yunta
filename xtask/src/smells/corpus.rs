//! How a measurement lists a directory. Every counter that walks the tree
//! goes through here, so a directory it cannot list stops the measure.

use std::path::{Path, PathBuf};

/// The entries of `dir`, in no particular order. A directory that does not
/// exist has none; any other failure stops the measure, because a measure
/// that cannot list a directory cannot report it clean.
pub(super) fn entries(dir: &Path) -> Vec<PathBuf> {
    let listing = match std::fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("the ratchet cannot list `{}`: {error}", dir.display()),
    };
    listing
        .map(|entry| match entry {
            Ok(entry) => entry.path(),
            Err(error) => panic!("the ratchet cannot list `{}`: {error}", dir.display()),
        })
        .collect()
}
