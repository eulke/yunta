//! A run a test holds on its first node until it lets it go. The node says when it started,
//! then waits for the test's word to go on, and ends anyway once the test's marks are gone,
//! so no engine outlives the test that started it. A second node follows, so a run let go
//! has more to say before it finishes.

use std::path::Path;
use std::process::{Child, Stdio};

use tempfile::TempDir;

use crate::{run_ids, stderr, wait_until, Checkout};

/// The workflow of a held run: its first node writes `started`, then holds until `go`
/// stands or `started` is gone; then one that finishes.
fn holding(started: &Path, go: &Path) -> String {
    format!(
        "name: hold\nnodes:\n  - id: hold\n    kind: bash\n    run: \"echo in > {0}; until [ -f {1} ] || [ ! -e {0} ]; do sleep 0.05; done\"\n  - {{ id: after, kind: bash, run: \"true\", depends_on: [hold] }}\n",
        started.display(),
        go.display()
    )
}

/// A run held on its first node, in a repository of its own. Not its engine: how it was
/// started says what runs it.
pub struct HeldRun {
    pub checkout: Checkout,
    /// Where the run's markers are, which a test may write beside.
    marks: TempDir,
    /// The run's whole id.
    pub id: String,
}

impl HeldRun {
    /// The run `bin` starts off a terminal, `yunta run` itself its engine, once its first
    /// node says it started: the run, and that process.
    pub fn started(bin: &Path) -> (HeldRun, Child) {
        let (checkout, marks) = repository();
        let process = checkout
            .command(bin)
            .args(["run", "wf.yaml"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the run starts");
        (once_started(checkout, marks), process)
    }

    /// The run `bin` starts detached, its engine a process of its own that `yunta run
    /// --detach` hands it to, once its first node says it started.
    pub fn started_detached(bin: &Path) -> HeldRun {
        let (checkout, marks) = repository();
        let detached = checkout.run(bin, &["run", "wf.yaml", "--detach"]);
        assert!(detached.status.success(), "{}", stderr(&detached));
        once_started(checkout, marks)
    }

    /// The run let go on from its first node.
    pub fn let_go(&self) {
        std::fs::write(self.marks.path().join("go"), "").expect("the word to go on");
    }

    /// Where the run's markers are, which a test may write beside.
    pub fn marks(&self) -> &Path {
        self.marks.path()
    }
}

/// A repository whose one workflow holds, and the markers its run says and hears by.
fn repository() -> (Checkout, TempDir) {
    let marks = tempfile::tempdir().expect("a directory for the markers");
    let workflow = holding(&marks.path().join("started"), &marks.path().join("go"));
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", &workflow)
        .committed();
    (checkout, marks)
}

/// The run of `checkout`, once its first node says it started.
fn once_started(checkout: Checkout, marks: TempDir) -> HeldRun {
    let started = marks.path().join("started");
    wait_until(|| started.exists(), || "the run never started".to_string());
    let id = run_ids(&checkout.home).remove(0);
    HeldRun {
        checkout,
        marks,
        id,
    }
}
