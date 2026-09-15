//! The counters that measure where a pattern is written.
//!
//! Each of these names a capability, the sites that own it, and counts
//! every line that writes it somewhere else. The second site is the
//! defect: the clock enters through one boundary, git runs from one
//! module, a session request is built by the plan that knows what a
//! session needs. A copy elsewhere answers the same question a second
//! time, and the two answers drift apart in silence.
//!
//! The corpus is `crates/*/src` and `crates/*/tests`, and counting is by
//! line, so a contributor and CI see the number `grep -c` gives for the
//! same pattern over the same files.

use std::path::Path;

use Mark::{StructNamed, Text};

/// A counter: the name it carries in the baseline, and the function that
/// measures it against a workspace root.
pub type Counter = (&'static str, fn(&Path) -> usize);

/// Every counter this module measures.
pub const COUNTERS: &[Counter] = &[
    (
        "execute_run_outside_testkit",
        count_execute_run_outside_testkit,
    ),
    (
        "create_run_outside_testkit",
        count_create_run_outside_testkit,
    ),
    (
        "git_command_outside_git_rs",
        count_git_command_outside_git_rs,
    ),
    (
        "system_clock_outside_boundary",
        count_system_clock_outside_boundary,
    ),
    ("env_read_outside_boundary", count_env_read_outside_boundary),
    ("env_mutation_in_tests", count_env_mutation_in_tests),
    (
        "sleep_outside_mock_latency",
        count_sleep_outside_mock_latency,
    ),
    (
        "event_builder_outside_testkit",
        count_event_builder_outside_testkit,
    ),
    (
        "bench_struct_outside_testkit",
        count_bench_struct_outside_testkit,
    ),
    (
        "session_request_literal_outside_plan",
        count_session_request_literal_outside_plan,
    ),
    ("reason_built_by_format", count_reason_built_by_format),
];

/// What a line carries to count as a write of the pattern.
enum Mark {
    /// The line carries this text anywhere on it.
    Text(&'static str),
    /// The line declares a struct with this word in its name, whatever
    /// stands between the keyword and the word.
    StructNamed(&'static str),
}

impl Mark {
    /// Whether `line` writes the pattern. A comment does not: prose
    /// naming a mechanism is a reader being told about it, and a counter
    /// that rises when someone explains the rule measures the
    /// explanation.
    fn on(&self, line: &str) -> bool {
        if line.trim_start().starts_with("//") {
            return false;
        }
        match self {
            Text(text) => line.contains(text),
            StructNamed(word) => line
                .split_once("struct ")
                .is_some_and(|(_, name)| name.contains(word)),
        }
    }
}

/// Lines under `root` carrying any of `marks`, outside the files that
/// `owners` names.
///
/// An owner is a path fragment: a directory fragment ends in `/` so it
/// covers what is under it, and `*` stands for the rest of one path
/// segment, so `crates/testkit*/src/repo.rs` names that file in every
/// harness crate. A line carrying two marks counts once, the way a
/// line-oriented search counts it.
fn outside(root: &Path, marks: &[Mark], owners: &[&str]) -> usize {
    super::ratchet_corpus(root)
        .iter()
        .filter(|path| !owned(path, owners))
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| {
            text.lines()
                .filter(|line| marks.iter().any(|mark| mark.on(line)))
                .count()
        })
        .sum()
}

/// Whether `path` is one of the sites `owners` names.
fn owned(path: &Path, owners: &[&str]) -> bool {
    let shown = path.to_string_lossy().replace('\\', "/");
    owners.iter().any(|owner| match owner.split_once('*') {
        None => shown.contains(owner),
        Some((head, tail)) => shown.match_indices(head).any(|(at, _)| {
            let rest = &shown[at + head.len()..];
            let segment = rest.find('/').unwrap_or(rest.len());
            rest[segment..].starts_with(tail)
        }),
    })
}

/// A run executed outside the harness that owns the call.
///
/// Driving the engine by hand repeats the setup the harness performs —
/// the store, the clock, the process registry — and a repeat drifts from
/// it. `drive` is the command whose whole job is to execute one run.
fn count_execute_run_outside_testkit(root: &Path) -> usize {
    outside(
        root,
        &[Text("execute_run(")],
        &["crates/testkit/", "crates/cli/src/commands/drive.rs"],
    )
}

/// A run created outside the harness that owns the call.
///
/// Creating a run is where the plan is frozen and the baseline taken; a
/// second site that does it by hand freezes a different one. `run` is the
/// command whose whole job is to create one.
fn count_create_run_outside_testkit(root: &Path) -> usize {
    outside(
        root,
        &[Text("create_run(")],
        &["crates/testkit/", "crates/cli/src/commands/run.rs"],
    )
}

/// A git subprocess spawned outside the module that owns git.
///
/// Every git call needs the same answers — which directory, which
/// environment, which failure is a diagnostic — and a call written
/// somewhere else answers them again, usually with one of them missing.
fn count_git_command_outside_git_rs(root: &Path) -> usize {
    outside(
        root,
        &[Text("Command::new(\"git\")")],
        &["crates/engine/src/git.rs", "crates/testkit*/src/repo.rs"],
    )
}

/// The wall clock read below the boundary that injects it.
///
/// The clock enters once, at the shell, and travels as an injected
/// `Clock`. Read deeper in, it makes a decision depend on when it runs:
/// the state stops deriving from the log, and a replay of the same log
/// gives a different answer.
fn count_system_clock_outside_boundary(root: &Path) -> usize {
    outside(
        root,
        &[Text("SystemClock"), Text("Utc::now")],
        &[
            "crates/core/src/clock.rs",
            "crates/cli/src/main.rs",
            "crates/cli/src/context.rs",
        ],
    )
}

/// The process environment read below the boundary that reads it.
///
/// The environment enters once, at the shell, and travels as config. Read
/// deeper in, it is an input nothing declares: a test cannot set it, a
/// diagnostic cannot name it, and the same code behaves differently on
/// two machines.
fn count_env_read_outside_boundary(root: &Path) -> usize {
    outside(
        root,
        &[Text("std::env::var")],
        &["crates/cli/src/main.rs", "crates/core/src/config/env.rs"],
    )
}

/// A test that writes the process environment.
///
/// The environment is one variable shared by every test in the binary, so
/// a write reaches the tests running beside it and the failure lands in
/// one of them. What a test needs from the environment it injects.
fn count_env_mutation_in_tests(root: &Path) -> usize {
    outside(root, &[Text("env::set_var"), Text("env::remove_var")], &[])
}

/// A sleep outside the two places where waiting is the behaviour.
///
/// A sleep as synchronization is a guess about how long something takes:
/// it passes on a fast machine, fails on a loaded one, and costs its
/// duration either way. A condition is waited for explicitly. The mock
/// adapter's scripted latency and the harness's wait are the places where
/// the delay is the thing being built.
fn count_sleep_outside_mock_latency(root: &Path) -> usize {
    outside(
        root,
        &[Text("tokio::time::sleep"), Text("thread::sleep")],
        &[
            "crates/adapters/src/mock/script.rs",
            "crates/testkit*/src/wait.rs",
        ],
    )
}

/// An event builder written outside the support crate that owns one.
///
/// Building an event for a test means getting the sequence, the run and
/// the node right; a copy of that bookkeeping in a test file gets one of
/// them wrong the day the payload changes, and the test that reads the
/// log stops proving what it names.
fn count_event_builder_outside_testkit(root: &Path) -> usize {
    outside(root, &[Text("fn event(")], &["crates/testkit-core/"])
}

/// A bench or a clock declared outside the support crate.
///
/// Both are test infrastructure: a crate that grows its own is a second
/// harness, and the two disagree about what a run under test looks like
/// and what time it is.
fn count_bench_struct_outside_testkit(root: &Path) -> usize {
    outside(
        root,
        &[StructNamed("Bench"), StructNamed("Clock")],
        // The clock the product reads time from is declared where the
        // `Clock` trait is; every other clock in the workspace is a
        // test's, and belongs to the harness.
        &["crates/testkit*/", "crates/core/src/clock.rs"],
    )
}

/// A session request built as a literal outside the plan.
///
/// What a session receives — its prompt, its capabilities, its settings,
/// its working copy — is decided in one place, from the node and the
/// config. A literal written elsewhere is a session that misses whatever
/// the plan learned to add.
fn count_session_request_literal_outside_plan(root: &Path) -> usize {
    outside(
        root,
        &[Text("SessionRequest {")],
        &[
            "crates/engine/src/run/session_plan.rs",
            // The type's own declaration, and the request the adapters'
            // tests vary one field of.
            "crates/core/src/port/session.rs",
            "crates/adapters/tests/",
            "crates/testkit-core/src/adapter.rs",
        ],
    )
}

/// A reason, a summary, a cause or an applied policy built by `format!`
/// at the site that records it.
///
/// A field of the log holds what happened, with the values it happened
/// with, so a reader can group it, count it and render it. Formatted into
/// a sentence at the site, it becomes prose no consumer can take apart,
/// and the text for a human stops being produced once, at the edge.
fn count_reason_built_by_format(root: &Path) -> usize {
    outside(
        root,
        &[
            Text("reason: format!"),
            Text("summary: format!"),
            Text("cause: format!"),
            Text("policy_applied: format!"),
        ],
        &[],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_mark_reads_the_spelling_the_code_writes() {
        // The counters that stand at zero stand there because the tree
        // holds none of their pattern, not because the pattern cannot
        // match: these are the lines each of them is waiting for.
        assert!(Text("env::set_var").on("    std::env::set_var(\"HOME\", home);"));
        assert!(Text("env::remove_var").on("    env::remove_var(\"YUNTA_HOME\");"));
        assert!(Text("fn event(").on("pub fn event(self, payload: EventPayload) -> Self {"));
        assert!(!Text("fn event(").on("    let event = log.event(payload);"));
    }

    #[test]
    fn a_struct_mark_reads_the_name_and_not_the_line_before_the_keyword() {
        assert!(StructNamed("Bench").on("pub struct Bench {"));
        assert!(StructNamed("Clock").on("    struct FixedClock;"));
        assert!(!StructNamed("Bench").on("let bench = Bench::new();"));
    }

    #[test]
    fn an_owner_covers_a_directory_and_a_file_in_every_crate_a_star_names() {
        assert!(owned(
            Path::new("/w/crates/testkit/src/bench/mod.rs"),
            &["crates/testkit/"]
        ));
        assert!(!owned(
            Path::new("/w/crates/testkit-core/src/log.rs"),
            &["crates/testkit/"]
        ));
        // `crates/testkit*/src/repo.rs` names that file in either harness
        // crate, and nothing beside it.
        let pattern = ["crates/testkit*/src/repo.rs"];
        assert!(owned(Path::new("/w/crates/testkit/src/repo.rs"), &pattern));
        assert!(owned(
            Path::new("/w/crates/testkit-core/src/repo.rs"),
            &pattern
        ));
        assert!(!owned(Path::new("/w/crates/engine/src/repo.rs"), &pattern));
    }
}
