//! What of a recording is kept, and how: the log the engine exported, and the
//! run's directory without the engine's working copies, each written as if any
//! recording had made it.
//!
//! A recording repeats every event, but a few facts each event holds belong to
//! the place and the machine it was made on, and are written one way:
//!
//! - every path under the place it was made in names [`PLACE`], and the pool of
//!   checkouts there, which the engine names by a hash of that place's path,
//!   [`POOL`];
//! - the machine's own scratch directory names [`SCRATCH`], which every machine
//!   has;
//! - each manifest's hash is the hash of the manifest so written;
//! - the room the machine's disk had, where a run found too little of it,
//!   reads as [`FREE`];
//! - what the engine times on the machine's own clock, rather than the run's,
//!   reads as the run's clock timed it. The recording moves that clock only
//!   while every session and command waits on a word, and the commands it holds
//!   while it moves (the lint and the suite a node runs) carry their time as
//!   their node's span; no command whose `duration_ms` the log holds is held
//!   while the clock moves, so each reads `0`, the time the clock moved under
//!   it. Every rail call a session made falls in the histogram's first bucket.

use std::path::{Path, PathBuf};

use serde_json::Value;
use yunta_core::Manifest;

/// What stands for the place a run was recorded in, wherever a path the log
/// holds names it.
pub(crate) const PLACE: &str = "/designed";

/// What stands for the pool of checkouts the place holds.
const POOL: &str = "worktree-designed";

/// The scratch directory every machine has, and the one a machine whose own
/// lies elsewhere names beside it: what the kept log names for either.
const SCRATCH: &str = "/tmp";

/// What stands for the bytes the machine's disk had free, wherever the log
/// says how much room a run found: a round figure, since no machine's disk
/// has the same room free at two recordings.
const FREE: u64 = 42_000_000_000;

/// What of the run's directory is kept: what a reader of the run reads, and not
/// the copy of its log the engine exports, nor the engine's working indexes.
pub(crate) const KEPT: [&str; 5] = ["artifacts", "baseline", MANIFEST, "node-output", "objects"];

/// The manifest the run froze, which names the place it was made in.
pub(crate) const MANIFEST: &str = "manifest.yaml";

/// Where one recording was made, as its log and its files name it.
pub(crate) struct Made {
    /// The directory everything of the run stands under.
    pub(crate) place: PathBuf,
    /// The pool of checkouts the engine took the run's units from.
    pub(crate) pool: PathBuf,
    /// The manifest each run of the recording froze, as it was frozen there:
    /// the run's, and the one each run it made froze.
    pub(crate) manifests: Vec<Manifest>,
}

impl Made {
    /// `exported`, the engine's log of the recording's runs, as it is kept.
    pub(crate) fn log(&self, exported: &str) -> String {
        let placed = self.unplaced(exported);
        let mut log: String = placed
            .lines()
            .map(|line| unroomed(&unclocked(line)) + "\n")
            .collect();
        for manifest in &self.manifests {
            let written = serde_json::to_string(manifest).expect("a manifest, written");
            let kept: Value =
                serde_json::from_str(&self.unplaced(&written)).expect("a manifest, kept");
            log = log.replace(
                manifest.manifest_hash().as_str(),
                yunta_core::content_hash(&kept).as_str(),
            );
        }
        log
    }

    /// What [`KEPT`] names of `run`, the recorded run's directory: each file by
    /// its path under the run, and what it holds as it is kept.
    pub(crate) fn files(&self, run: &Path) -> Vec<(PathBuf, String)> {
        let mut files = Vec::new();
        for name in KEPT {
            gathered(run, Path::new(name), &mut files);
        }
        for (path, text) in &mut files {
            if path.as_path() == Path::new(MANIFEST) {
                *text = self.unplaced(text);
            }
            for place in places(&self.place) {
                assert!(
                    !text.contains(&place),
                    "`{}` names `{place}`, where the run was recorded",
                    path.display()
                );
            }
        }
        files
    }

    /// `text` with the place and the machine's scratch directory named as
    /// they are kept.
    fn unplaced(&self, text: &str) -> String {
        let mut kept = text.to_string();
        let pool = self
            .pool
            .file_name()
            .expect("a pool is a directory")
            .to_string_lossy()
            .into_owned();
        for place in places(&self.place) {
            kept = kept.replace(&place, PLACE);
        }
        kept = kept.replace(&pool, POOL);
        for scratch in places(&std::env::temp_dir()) {
            let quoted = format!("\"{scratch}\"");
            kept = kept
                .replace(
                    &format!("{quoted},\"{SCRATCH}\""),
                    &format!("\"{SCRATCH}\""),
                )
                .replace(&quoted, &format!("\"{SCRATCH}\""));
        }
        kept
    }
}

/// Every way a path may name `place`: as the file system resolves it, then as
/// it was made, the longer first where one holds the other.
fn places(place: &Path) -> Vec<String> {
    let made = place.display().to_string();
    let mut named = Vec::new();
    if let Ok(resolved) = place.canonicalize() {
        let resolved = resolved.display().to_string();
        if resolved != made {
            named.push(resolved);
        }
    }
    named.push(made);
    named
}

/// One event of the log, what the machine's own clock timed in it read as the
/// run's clock read it. An event that holds no such time is kept as written.
fn unclocked(line: &str) -> String {
    let rail = line.contains("\"kind\":\"rail_summary\"");
    if !rail && !line.contains("\"duration_ms\":") {
        return line.to_string();
    }
    let mut event: Value = serde_json::from_str(line).expect("an event of the log");
    untimed(&mut event, rail);
    serde_json::to_string(&event).expect("an event, written again")
}

/// One event of the log, the room a pause found on the machine's disk read as
/// [`FREE`]. An event that says no room is kept as written.
fn unroomed(line: &str) -> String {
    if !line.contains("\"no_room\"") {
        return line.to_string();
    }
    let mut event: Value = serde_json::from_str(line).expect("an event of the log");
    roomed(&mut event);
    serde_json::to_string(&event).expect("an event, written again")
}

/// `value` with the room every `no_room` in it found read as [`FREE`].
fn roomed(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if fields.get("type") == Some(&Value::from("no_room")) {
                fields.insert("available".to_string(), Value::from(FREE));
            }
            fields.values_mut().for_each(roomed);
        }
        Value::Array(items) => items.iter_mut().for_each(roomed),
        _ => {}
    }
}

/// `value` with every `duration_ms` at zero and, in a rail's summary, every
/// histogram's calls in its first bucket.
fn untimed(value: &mut Value, rail: bool) {
    match value {
        Value::Object(fields) => {
            if rail {
                if let (Some(count), Some(buckets)) =
                    (fields.get("count").cloned(), fields.get_mut("buckets"))
                {
                    *buckets = serde_json::json!({ "0": count });
                }
            }
            for (key, field) in fields.iter_mut() {
                if key == "duration_ms" && field.is_number() {
                    *field = Value::from(0);
                } else if key != "buckets" {
                    untimed(field, rail);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| untimed(item, rail)),
        _ => {}
    }
}

/// Every file at `path` under `run`, a file or a directory, each with its text,
/// added to `files` in the order of their paths.
fn gathered(run: &Path, path: &Path, files: &mut Vec<(PathBuf, String)>) {
    let at = run.join(path);
    if !at.is_dir() {
        let text = std::fs::read_to_string(&at)
            .unwrap_or_else(|error| panic!("`{}` of the run: {error}", path.display()));
        files.push((path.to_path_buf(), text));
        return;
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&at)
        .expect("a directory of the run")
        .map(|entry| {
            entry
                .expect("an entry of the run's directory")
                .file_name()
                .into()
        })
        .collect();
    entries.sort();
    for entry in entries {
        gathered(run, &path.join(entry), files);
    }
}
