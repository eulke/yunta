//! The engine is a library a shell drives, not a program that reaches
//! for the machine on its own.
//!
//! Clock, environment and disk are the three ways a module reaches past
//! its arguments. Each one taken directly makes the engine untestable
//! without the machine it runs on: a run cannot be replayed against a
//! fixed clock, a secret cannot be injected, and a `std::fs` call inside
//! an async task blocks the executor that was supposed to be running the
//! rest of the run.
//!
//! `process.rs` is the exception for the machine it must touch: it is
//! the shell module — it spawns children, signals them and waits on
//! them — so what it touches it touches on purpose, before a child
//! exists or after one is gone. The clock is not among them: a
//! supervision carries the one it tells the time by, so no module of
//! this crate reads the process clock.
//!
//! These read the source. A grep test is coarse, and coarse is what
//! keeps it honest: it cannot be satisfied by anything but not doing the
//! thing.

use std::path::{Path, PathBuf};

/// Where the engine's own source lives.
fn engine_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` file under `dir`, recursively, with its path relative to
/// the crate's `src`.
fn sources(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let entries = std::fs::read_dir(dir).expect("the engine's own source is readable");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let relative = path
                .strip_prefix(root)
                .expect("every source is under src")
                .display()
                .to_string();
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            out.push((relative, production_only(&text)));
        }
    }
}

/// What marks the one kind of line this cannot be right about: a
/// context with no `await` to give, such as a `Drop`. Writing it is the
/// whole point — an exception nobody can take without saying why.
const JUSTIFIED: &str = "// blocking:";

/// Every line of engine source outside `exempt`, as `(file, line no, line)`.
fn lines_outside(exempt: &[&str]) -> Vec<(String, usize, String)> {
    let root = engine_src();
    let mut files = Vec::new();
    sources(&root, &root, &mut files);
    files
        .into_iter()
        .filter(|(path, _)| !exempt.iter().any(|allowed| path.starts_with(allowed)))
        .flat_map(|(path, text)| {
            text.lines()
                .enumerate()
                .map(|(index, line)| (path.clone(), index + 1, line.to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `text` with every `#[cfg(test)]` module blanked out: what a test
/// fixture reaches for is the test's business, not the engine's.
fn production_only(text: &str) -> String {
    let mut out = Vec::new();
    let mut depth: Option<i32> = None;
    for line in text.lines() {
        if depth.is_none() && line.trim_start().starts_with("#[cfg(test)]") {
            depth = Some(0);
        }
        match &mut depth {
            None => out.push(line.to_string()),
            Some(open) => {
                out.push(String::new());
                *open += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                if *open <= 0 && line.contains('}') {
                    depth = None;
                }
            }
        }
    }
    out.join("\n")
}

/// The engine's own shell: the modules that own subprocesses and the
/// bookkeeping about them — what was spawned, what is still alive, what
/// an outside `yunta cancel` may signal. They reach the machine on
/// purpose, at spawn and at exit, where there is no run to block.
const SHELL: [&str; 3] = ["process.rs", "process/", "process_registry.rs"];

/// Modules with no run and no executor around them: they answer a
/// question the CLI asked on its own thread — which workflows exist,
/// what a pack declares, whether a reference resolves — before any run
/// is born. Blocking there blocks nothing but the caller that asked.
const NO_RUNTIME: [&str; 3] = ["catalog.rs", "check/", "pack_audit.rs"];

/// Sites of `needle` outside the shell and outside a justified
/// exception, rendered for a failure message.
fn offenders(needle: &str) -> Vec<String> {
    offenders_outside(needle, &SHELL)
}

/// Sites of `needle` outside `exempt` and outside a justified
/// exception. The clock has no exemption at all: even the shell tells
/// the time by what it is handed.
fn offenders_outside(needle: &str, exempt: &[&str]) -> Vec<String> {
    let lines = lines_outside(exempt);
    // A marker covers the first line of code under it, however many
    // lines the justification itself takes.
    let mut justified: std::collections::HashSet<(String, usize)> =
        std::collections::HashSet::new();
    let mut pending: Option<String> = None;
    for (path, number, line) in &lines {
        if line.contains(JUSTIFIED) {
            pending = Some(path.clone());
        } else if !line.trim_start().starts_with("//") {
            if pending.as_deref() == Some(path.as_str()) {
                justified.insert((path.clone(), *number));
            }
            pending = None;
        }
    }
    lines
        .iter()
        .filter(|(_, _, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, _, line)| line.contains(needle))
        .filter(|(path, number, _)| !justified.contains(&(path.clone(), *number)))
        .map(|(path, number, line)| format!("{path}:{number}: {}", line.trim()))
        .collect()
}

/// The clock is injected. A module that reads the process clock derives
/// a different run every time it replays one.
///
/// No exemption, the shell included: a supervision carries the clock it
/// tells the time by, so nothing below the caller has a reason to ask
/// the machine.
///
/// `Instant` is not the clock: it measures how long something took,
/// which is a fact about this machine's execution and not a timestamp a
/// replay has to reproduce.
#[test]
fn no_engine_module_reads_the_process_clock() {
    for needle in ["Utc::now", "SystemClock"] {
        let found = offenders_outside(needle, &[]);
        assert!(
            found.is_empty(),
            "`{needle}` belongs to the shell that builds the engine's clock, \
             not to the engine:\n  {}",
            found.join("\n  ")
        );
    }
}

/// The environment is injected. A module that reads it directly cannot
/// be handed a secret, and a test cannot take one away.
#[test]
fn no_engine_module_reads_the_process_environment() {
    let found = offenders("std::env::var");
    assert!(
        found.is_empty(),
        "the environment arrives through `Env` and `SecretSource`:\n  {}",
        found.join("\n  ")
    );
}

/// Disk is async or explicitly blocking. A `std::fs` call inside an
/// async task blocks the executor that was running the rest of the run.
#[test]
fn no_engine_module_blocks_the_executor_on_disk() {
    let found: Vec<String> = offenders("std::fs::")
        .into_iter()
        .filter(|site| !NO_RUNTIME.iter().any(|allowed| site.starts_with(allowed)))
        .collect();
    assert!(
        found.is_empty(),
        "disk goes through `tokio::fs` or `spawn_blocking`:\n  {}",
        found.join("\n  ")
    );
}

/// Every git the engine runs is a subprocess the caller owns: born in
/// its own process group, registered, killed with its whole tree when
/// the token fires. `git.rs` is the one door, so a `Command::new("git")`
/// anywhere else is a git nobody governs.
#[test]
fn no_engine_module_spawns_git_outside_the_shell() {
    let found = offenders_outside("Command::new(\"git\")", &["git.rs"]);
    assert!(
        found.is_empty(),
        "git goes through `git::{{output, success, output_bytes}}`, which spawn it \
         governed:\n  {}",
        found.join("\n  ")
    );
}
