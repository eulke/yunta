//! Where a run is recorded: a repository holding what it held before the run,
//! committed at a fixed instant; the `cargo` and the `git` its commands find
//! first on their `PATH`; the words its sessions and commands wait for; and
//! the bench that drives it, on a clock the recording moves. Every commit the
//! engine makes there is dated by the same instant and reads none of the
//! machine's git configuration, so a tree and a commit read the same in every
//! recording. The design's run is recorded in [`Place::made`], whose
//! repository holds what the pack commands looked like before it; a small run
//! in a place [`Place::of`] makes with the files it names.
//!
//! Nodes the run starts together race to the log as they start, so each is
//! held before its first event: the engine opens a node that only reads a
//! checkout of its own, through an index of its own, and the `git` here holds
//! the first call made through a held node's index until its word is given.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_testkit::{bare_origin, git, git_output, Bench, Hold};
use yunta_testkit_core::{fixed_now, HostClock};

use super::forge::DesignedForge;
use super::{IDEA, RUN};

/// The instant every commit of the run's repository is dated by: the day the
/// repository's last commit before the run was made, as a commit records it.
const COMMITTED_AT: &str = "1767139200 +0000";

/// Who made the repository's commit before the run, as the bench's repository
/// names its author.
const AUTHOR: &str = "Test <test@example.com>";

/// The files the design's repository holds before the run, each with what it
/// says.
const BEFORE: &[(&str, &str)] = &[
    (
        "docs/pack.md",
        "# Packs\n\nA pack is installed into the project that adds it: `yunta pack add` \
         vendors it under `.yunta/packs/` and records it in `.yunta/yunta.lock`.\n",
    ),
    (
        "crates/cli/src/pack.rs",
        "//! Packs: where they are installed, and the lock that records them.\n\n\
         use std::path::{Path, PathBuf};\n\n\
         /// The directory the project's packs are vendored under.\n\
         pub fn packs_root(cwd: &Path) -> PathBuf {\n    cwd.join(\".yunta/packs\")\n}\n\n\
         /// The lock the project's installs are recorded in.\n\
         pub fn lock_path(cwd: &Path) -> PathBuf {\n    cwd.join(\".yunta/yunta.lock\")\n}\n",
    ),
    (
        "crates/cli/src/cli/pack.rs",
        "//! `yunta pack`'s arguments.\n\n\
         pub(super) enum PackAction {\n    Add { source: String, yes: bool, run_tests: bool },\n    \
         Update { pack: PackRef, r#ref: String, yes: bool },\n    New { pack: PackRef },\n    \
         Remove { pack: PackRef },\n    List,\n    Audit { pack: PackRef },\n}\n",
    ),
    (
        "crates/cli/src/cli/dispatch.rs",
        "//! Which command an invocation runs.\n\n\
         pub(super) fn pack(action: PackAction, look: &Look) -> Dispatched {\n    match action {\n        \
         PackAction::Add { source, yes, run_tests } => {\n            \
         Dispatched::later(commands::pack::add(source, yes, run_tests, look))\n        }\n        \
         PackAction::Remove { pack } => Dispatched::now(commands::pack::remove(&pack, look)),\n        \
         PackAction::List => Dispatched::now(commands::pack::list(look)),\n        \
         other => Dispatched::now(commands::pack::other(other, look)),\n    }\n}\n",
    ),
    (
        "crates/cli/src/commands/pack.rs",
        "//! `yunta pack`: add, update, remove and list, on the project.\n\n\
         pub async fn add(source: &str, confirmed_executors: bool, run_tests: bool, look: &Look) -> \
         Result<Outcome, CliError> {\n    install(source, confirmed_executors, run_tests, \
         &packs_root(&look.cwd), look).await\n}\n\n\
         pub fn remove(pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {\n    \
         uninstall(pack, &packs_root(&look.cwd), look)\n}\n\n\
         pub fn list(look: &Look) -> Result<Outcome, CliError> {\n    \
         listed(&lock_path(&look.cwd), look)\n}\n",
    ),
    (
        "crates/cli/src/commands/pack_audit.rs",
        "//! `yunta pack audit`, on the project's store.\n\n\
         pub fn audit(pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {\n    \
         audited(&packs_root(&look.cwd).join(pack.as_path()), look)\n}\n",
    ),
    (
        "crates/cli/tests/pack_cmd.rs",
        "//! `yunta pack` on a project.\n\n#[test]\nfn a_pack_added_is_listed() {\n    \
         let checkout = Checkout::new().committed();\n    \
         checkout.yunta(&[\"pack\", \"add\", \"acme/demo\", \"--yes\"]);\n    \
         assert!(checkout.yunta(&[\"pack\", \"list\"]).stdout.contains(\"acme/demo\"));\n}\n",
    ),
];

/// The `cargo` the run's commands find: the suite, the task's test and the lint,
/// each answering from what the run's tree holds. A command the recording armed
/// says it is held and waits for its word, as a session waits for one.
const CARGO: &str = r#"#!/bin/sh
words='@words@'
held() {
  if [ -f "$words/arm-$1" ]; then
    rm -f "$words/arm-$1"
    : > "$words/held-$1"
    until [ -f "$words/$1" ] || [ ! -d "$words" ]; do sleep 0.02; done
    rm -f "$words/$1" "$words/held-$1"
  fi
}
case "$1" in
test)
  if [ $# -gt 2 ]; then
    for selected; do :; done
    if grep -q 'enum PackTarget' crates/cli/src/pack.rs crates/cli/src/pack/target.rs 2>/dev/null \
      && grep -q 'PackTarget' crates/cli/src/commands/pack_audit.rs 2>/dev/null \
      && [ -f crates/cli/src/pack/target.rs ]; then
      echo "running 1 test"
      echo "test $selected ... ok"
      echo "test result: ok. 1 passed; 0 failed; 0 ignored; finished in 4.02s"
      exit 0
    fi
    echo "running 1 test"
    echo "test $selected ... FAILED"
    echo "test result: FAILED. 0 passed; 1 failed; 0 ignored; finished in 3.87s"
    exit 101
  fi
  held suite
  if [ -f "$words/suite-fails" ]; then
    rm -f "$words/suite-fails"
    echo "---- smells::tests::the_tree_holds_each_counter_that_reaches_zero_at_zero stdout ----"
    echo "panicked at xtask/src/smells.rs:473:13:"
    echo "assertion \`left == right\` failed: src_files_over_500_lines"
    echo "  left: Some(1)"
    echo " right: Some(0)"
    echo "test result: FAILED. 71 passed; 1 failed; 0 ignored; finished in 21.18s"
    exit 101
  fi
  echo "test result: ok. 72 passed; 0 failed; 0 ignored; finished in 21.18s"
  exit 0
  ;;
clippy)
  held clippy
  if grep -q 'fn packs_root' crates/cli/src/pack.rs 2>/dev/null \
    && grep -q 'enum PackTarget' crates/cli/src/pack.rs 2>/dev/null; then
    echo "error: function \`packs_root\` is never used"
    echo "  --> crates/cli/src/pack.rs:31:4"
    echo "error: function \`vendor_dir\` is never used"
    echo "  --> crates/cli/src/pack.rs:35:4"
    echo "error: could not compile \`yunta\` (lib) due to 2 previous errors"
    exit 101
  fi
  echo "    Finished \`dev\` profile [unoptimized + debuginfo] in 11.02s"
  exit 0
  ;;
esac
exit 0
"#;

/// The `git` the engine finds: the machine's own, but that the first call made
/// through the index of a unit the recording holds waits for its word, and
/// that while the recording holds the pool's surveys, each survey waits for
/// the word naming the process that makes it.
const GIT: &str = r#"#!/bin/sh
words='@words@'
case "$GIT_INDEX_FILE" in
*.commit)
  unit=$(basename "$GIT_INDEX_FILE" .commit)
  if [ -f "$words/arm-$unit" ]; then
    rm -f "$words/arm-$unit"
    : > "$words/held-$unit"
    until [ -f "$words/$unit" ] || [ ! -d "$words" ]; do sleep 0.02; done
    rm -f "$words/$unit" "$words/held-$unit"
  fi
  ;;
esac
case " $* " in
*" worktree list --porcelain "*)
  if [ -f "$words/arm-surveys" ]; then
    : > "$words/held-survey-$$"
    until [ -f "$words/survey-$$" ] || [ ! -d "$words" ]; do sleep 0.02; done
    rm -f "$words/survey-$$" "$words/held-survey-$$"
  fi
  ;;
esac
exec '@git@' "$@"
"#;

/// The directories the machine's own `git` is looked for in, as the run's
/// commands look for programs past the ones placed for it.
const SYSTEM: [&str; 2] = ["/bin", "/usr/bin"];

/// The world a run is recorded in.
pub(crate) struct Place {
    pub(crate) bench: Bench,
    /// The run's clock, which stands still until the recording moves it.
    pub(crate) clock: Arc<HostClock>,
    /// The words the run's sessions and commands wait for.
    words: Hold,
}

impl Place {
    /// The place the design's run is recorded in: the repository as it stood
    /// before the run, on the forge the run opens its pull request on, the run
    /// asked for the design's idea in fragua's `standard` mode.
    pub(super) fn made() -> Place {
        Place::of(RUN, BEFORE, |bench| {
            bench
                .with_forge(Arc::new(DesignedForge))
                .with_inputs(&[("idea", IDEA)])
                .in_mode("standard")
        })
    }

    /// A fresh place for the run `run`: a repository holding `before`'s files,
    /// each a path and what it says, its origin, the run's `cargo` and `git`,
    /// and the bench, `configured` as the run needs it, which has not created
    /// the run yet. Its clock starts at the instant the design's run starts at.
    pub(crate) fn of(
        run: &str,
        before: &[(&str, &str)],
        configured: impl FnOnce(Bench) -> Bench,
    ) -> Place {
        Place::starting(fixed_now(), run, before, configured)
    }

    /// A fresh place for the run `run`, as [`Place::of`] makes it, its clock
    /// starting at `start`: a run made before or after the design's.
    pub(crate) fn starting(
        start: DateTime<Utc>,
        run: &str,
        before: &[(&str, &str)],
        configured: impl FnOnce(Bench) -> Bench,
    ) -> Place {
        let words = Hold::new();
        let clock = Arc::new(HostClock::at(start));
        let bench = Bench::with_run_id(run);
        let root = bench
            .worktree
            .parent()
            .expect("a bench's worktree stands in its root")
            .to_path_buf();
        let bin = programs(&root, words.marks());
        repository(&bench.worktree, &root, before);
        bare_origin(&bench.worktree);
        let dated = format!("@{COMMITTED_AT}");
        let vars = vec![
            (
                "PATH".to_string(),
                format!("{}:{}", bin.display(), SYSTEM.join(":")),
            ),
            ("GIT_AUTHOR_DATE".to_string(), dated.clone()),
            ("GIT_COMMITTER_DATE".to_string(), dated),
            ("GIT_CONFIG_GLOBAL".to_string(), "/dev/null".to_string()),
            ("GIT_CONFIG_SYSTEM".to_string(), "/dev/null".to_string()),
        ];
        let bench = configured(bench.with_clock(clock.clone()).with_subprocess_vars(vars));
        Place {
            bench,
            clock,
            words,
        }
    }

    /// The directory everything of the run stands under.
    pub(crate) fn root(&self) -> &Path {
        self.bench
            .worktree
            .parent()
            .expect("a bench's worktree stands in its root")
    }

    /// The mock's fixture, its words given where this place gives them.
    pub(crate) fn fixture(&self, fixture: &str) -> String {
        with_words(fixture, self.words.marks())
    }

    /// Moves the run's clock on by `seconds`, its wall and its awake time
    /// together.
    pub(crate) fn moves(&self, seconds: u64) {
        self.clock.advance(Duration::from_secs(seconds));
    }

    /// Gives the word `name`, so what waits for it goes on.
    pub(crate) fn give(&self, name: &str) {
        self.words.give(name);
    }

    /// Makes the next run of the command `name` hold until its word is given.
    pub(crate) fn arm(&self, name: &str) {
        self.words.give(&format!("arm-{name}"));
    }

    /// Lets every survey of the pool go on, held or made from now on.
    pub(crate) fn disarm(&self, name: &str) {
        // best-effort: an arm the command took already is gone, as it should be
        let _ = std::fs::remove_file(self.words.word(&format!("arm-{name}")));
    }

    /// The process of each survey of the pool held, waiting for its word, the
    /// first made first: a process the host made later has a higher number.
    pub(crate) fn held_surveys(&self) -> Vec<u32> {
        let mut held: Vec<u32> = std::fs::read_dir(self.words.marks())
            .expect("the words' directory")
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().into_string().ok()?;
                name.strip_prefix("held-survey-")?.parse().ok()
            })
            .collect();
        held.sort_unstable();
        held
    }

    /// Lets the survey the process `pid` makes go on.
    pub(crate) fn let_survey(&self, pid: u32) {
        // best-effort: the survey takes its word and removes its mark itself
        let _ = std::fs::remove_file(self.words.word(&format!("held-survey-{pid}")));
        self.words.give(&format!("survey-{pid}"));
    }

    /// Makes the next run of the suite fail.
    pub(crate) fn fails_once(&self) {
        self.words.give("suite-fails");
    }

    /// Whether the command `name` is held, waiting for its word.
    pub(crate) fn holds(&self, name: &str) -> bool {
        self.words.word(&format!("held-{name}")).exists()
    }

    /// Whether checkout `number` of the run's pool stands given back: no hold of
    /// it stands, its lock let go.
    pub(crate) fn given_back(&self, number: u32) -> bool {
        !self
            .bench
            .pool()
            .join(format!("slot-{number}.lock"))
            .exists()
    }

    /// The name the start of `unit` is held by: the index the engine opens its
    /// checkout through is named by the hash of the unit's name, `node/<id>`
    /// for a node.
    pub(crate) fn start_of(unit: &str) -> String {
        yunta_core::sha256_hex(unit.as_bytes()).as_str().to_string()
    }
}

/// `text` with every `@words@` naming where `words` are given.
fn with_words(text: &str, words: &Path) -> String {
    text.replace("@words@", &words.display().to_string())
}

/// The run's `cargo` and `git`, written under `root`, waiting for the words
/// given in `words`: the directory they stand in.
fn programs(root: &Path, words: &Path) -> std::path::PathBuf {
    let bin = root.join("bin");
    write_executable(&bin.join("cargo"), &with_words(CARGO, words));
    let git_of_the_machine = SYSTEM
        .iter()
        .map(|dir| Path::new(dir).join("git"))
        .find(|git| git.exists())
        .expect("git stands on the machine");
    let shim = with_words(GIT, words).replace("@git@", &git_of_the_machine.display().to_string());
    write_executable(&bin.join("git"), &shim);
    bin
}

/// The repository `worktree` holds before the run: `before`'s files, in the
/// one commit its history starts from, written whole as git writes a commit
/// so it is the same commit wherever it is made. `root` holds the commit
/// while it is written.
fn repository(worktree: &Path, root: &Path, before: &[(&str, &str)]) {
    for (path, content) in before {
        let file = worktree.join(path);
        std::fs::create_dir_all(file.parent().expect("a file stands in a directory"))
            .expect("the repository's directories");
        std::fs::write(file, content).expect("the repository's files");
    }
    git(worktree, &["add", "-A"]);
    let tree = git_output(worktree, &["write-tree"]);
    let commit = root.join("initial.commit");
    std::fs::write(
        &commit,
        format!("tree {tree}\nauthor {AUTHOR} {COMMITTED_AT}\ncommitter {AUTHOR} {COMMITTED_AT}\n\ninitial\n"),
    )
    .expect("the commit, written");
    let commit = commit.display().to_string();
    let initial = git_output(worktree, &["hash-object", "-t", "commit", "-w", &commit]);
    git(worktree, &["update-ref", "HEAD", &initial]);
}

/// `content` written at `path` as a program its owner runs.
fn write_executable(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().expect("a program stands in a directory"))
        .expect("a directory for the program");
    std::fs::write(path, content).expect("the program");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("a program that runs");
}
