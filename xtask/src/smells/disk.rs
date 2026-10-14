//! The counters over disk calls and over results a call discards. A
//! synchronous call inside an async body holds the thread the runtime gave
//! the task; a disk call in the engine needs a reason written above it; a
//! `let _ =` binding drops a failure no one reads.

use std::path::Path;

use super::corpus::{crate_src_dirs, is_test_source, rs_files};
use super::read_corpus;
use super::shape::{production_only, sync_fs_in_async};

/// A synchronous file call inside an async body: it holds the thread the
/// runtime gave the task until the disk answers. Production source only.
pub(super) fn count_sync_fs_in_async(root: &Path) -> usize {
    crate_src_dirs(root)
        .iter()
        .flat_map(|dir| rs_files(dir))
        .filter(|path| !is_test_source(path))
        .map(|path| sync_fs_in_async(&production_only(&read_corpus(&path))))
        .sum()
}

/// Discarded results in production text: a `let _ =` binding, or a `.ok();`
/// statement that is not an assignment, unless the line above is a
/// `// best-effort:` marker.
pub(super) fn discarded_results(text: &str) -> usize {
    let lines: Vec<&str> = text.lines().collect();
    (0..lines.len())
        .filter(|&at| {
            let line = lines[at].trim_start();
            line.starts_with("let _ =") || (line.ends_with(".ok();") && !line.contains(" = "))
        })
        .filter(|&at| !(at > 0 && lines[at - 1].trim_start().starts_with("// best-effort:")))
        .count()
}

/// Disk calls in the engine's production source, outside the shell and the
/// modules that run without a runtime, unless the first line of code under
/// a `// blocking:` comment is the call.
pub(super) fn count_engine_fs_outside_runtime(root: &Path) -> usize {
    const SHELL: [&str; 3] = ["process.rs", "process/", "process_registry.rs"];
    const NO_RUNTIME: [&str; 3] = ["catalog.rs", "check/", "pack_audit.rs"];
    let src = root.join("crates/engine/src");
    rs_files(&src)
        .iter()
        .filter(|path| !is_test_source(path))
        .filter_map(|path| {
            let relative = path.strip_prefix(&src).ok()?.to_string_lossy().into_owned();
            if SHELL
                .iter()
                .chain(&NO_RUNTIME)
                .any(|exempt| relative.starts_with(exempt))
            {
                return None;
            }
            let text = production_only(&std::fs::read_to_string(path).ok()?);
            Some(unjustified_disk_calls(&text))
        })
        .sum()
}

/// Lines of `text` that call `std::fs::` and are not the first line of code
/// under a `// blocking:` comment.
fn unjustified_disk_calls(text: &str) -> usize {
    let lines: Vec<&str> = text.lines().collect();
    let mut justified = std::collections::HashSet::new();
    let mut pending = false;
    for (at, line) in lines.iter().enumerate() {
        if line.contains("// blocking:") {
            pending = true;
        } else if pending && !line.trim_start().starts_with("//") {
            justified.insert(at);
            pending = false;
        }
    }
    lines
        .iter()
        .enumerate()
        .filter(|(at, line)| {
            line.contains("std::fs::")
                && !line.trim_start().starts_with("//")
                && !justified.contains(at)
        })
        .count()
}
