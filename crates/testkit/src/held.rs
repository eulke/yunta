//! A run a test holds on its first node until it lets it go. The node says when it started,
//! then waits for the test's word to go on, and ends anyway once the test's marks are gone,
//! so no engine outlives the test that started it. A second node follows, so a run let go
//! has more to say before it finishes.

use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};

use tempfile::TempDir;

use crate::{run_ids, stderr, wait_until, Checkout};

/// What a held run's first node says and hears by: the marker it writes once it started,
/// and the word it waits for to go on. Dropped, it gives that word, however the test ends:
/// a held engine started at a terminal stays in the terminal's session and keeps it open,
/// so it has to end before the terminal is dropped, which waits for it to close. Not a
/// run: the workflow that holds on it, which a test or the application starts.
pub struct Hold {
    marks: TempDir,
}

impl Hold {
    /// Marks no run has said or heard by yet.
    pub fn new() -> Hold {
        Hold {
            marks: tempfile::tempdir().expect("a directory for the markers"),
        }
    }

    /// The workflow `hold`: its first node writes that it started, then holds until the
    /// word to go on stands or the marks are gone; then a node that finishes.
    pub fn workflow(&self) -> String {
        self.holding("")
    }

    /// The workflow [`Hold::workflow`] says, taking what a person asks for as its request,
    /// the input `ask`.
    pub fn workflow_taking_a_request(&self) -> String {
        self.holding("inputs:\n  ask: { type: string, request: true }\n")
    }

    /// Whether the run's first node said it started.
    pub fn started(&self) -> bool {
        self.started_marker().exists()
    }

    /// The run let go on from its first node.
    pub fn let_go(&self) {
        std::fs::write(self.go(), "").expect("the word to go on");
    }

    /// Where the run's markers are, which a test may write beside.
    pub fn marks(&self) -> &Path {
        self.marks.path()
    }

    /// The workflow `hold`, with `inputs` written under its name.
    fn holding(&self, inputs: &str) -> String {
        format!(
            "name: hold\n{inputs}nodes:\n  - id: hold\n    kind: bash\n    run: \"echo in > {0}; until [ -f {1} ] || [ ! -e {0} ]; do sleep 0.05; done\"\n  - {{ id: after, kind: bash, run: \"true\", depends_on: [hold] }}\n",
            self.started_marker().display(),
            self.go().display()
        )
    }

    /// The marker the run's first node writes once it started.
    fn started_marker(&self) -> PathBuf {
        self.marks.path().join("started")
    }

    /// The word the run's first node waits for to go on.
    fn go(&self) -> PathBuf {
        self.marks.path().join("go")
    }
}

impl Default for Hold {
    fn default() -> Self {
        Hold::new()
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        // best-effort: the marks go with the hold, which ends its node anyway.
        let _ = std::fs::write(self.go(), "");
    }
}

/// A run held on its first node, in a repository of its own. Not its engine: how it was
/// started says what runs it.
pub struct HeldRun {
    pub checkout: Checkout,
    /// What the run's first node says and hears by.
    pub hold: Hold,
    /// The run's whole id.
    pub id: String,
}

impl HeldRun {
    /// The run `bin` starts off a terminal, `yunta run` itself its engine, once its first
    /// node says it started: the run, and that process.
    pub fn started(bin: &Path) -> (HeldRun, Child) {
        let (checkout, hold) = repository();
        let process = checkout
            .command(bin)
            .args(["run", "wf.yaml"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the run starts");
        (once_started(checkout, hold), process)
    }

    /// The run `bin` starts detached, its engine a process of its own that `yunta run
    /// --detach` hands it to, once its first node says it started.
    pub fn started_detached(bin: &Path) -> HeldRun {
        let (checkout, hold) = repository();
        let detached = checkout.run(bin, &["run", "wf.yaml", "--detach"]);
        assert!(detached.status.success(), "{}", stderr(&detached));
        once_started(checkout, hold)
    }
}

/// A repository whose one workflow holds, and what its run says and hears by.
fn repository() -> (Checkout, Hold) {
    let hold = Hold::new();
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", &hold.workflow())
        .committed();
    (checkout, hold)
}

/// The run of `checkout`, once its first node says it started.
fn once_started(checkout: Checkout, hold: Hold) -> HeldRun {
    wait_until(|| hold.started(), || "the run never started".to_string());
    let id = run_ids(&checkout.home).remove(0);
    HeldRun { checkout, hold, id }
}
