//! The resume an answer starts, as whoever answered meets it: `resolve-gate` says the
//! answer is resolved and the run moves on from there, with nobody resuming it by hand.

use yunta_core::events::{EventPayload, GateEvent};
use yunta_core::RunId;
use yunta_storage::Storage;
use yunta_testkit::{full_run_id, runs_root, unwrapped};

use super::*;

/// A gate with a node after it, which only an answer that reached an engine starts.
const GATED: &str = r#"
name: gated
nodes:
  - id: approve
    kind: gate
    assignee: lead
  - id: work
    kind: bash
    depends_on: [approve]
    run: "true"
"#;

/// A run of [`GATED`] parked on its gate, with its home and its whole id.
struct Parked {
    _root: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
    run_id: String,
}

impl Parked {
    fn on_its_gate() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = repo_with(root.path(), &[("gated", GATED)]);
        let home = root.path().join("state");
        let run = yunta_in!(&repo, &home, &["run", "gated.yaml"]);
        let run_id = full_run_id(&home, &run_id_from(&run));
        Parked {
            _root: root,
            repo,
            home,
            run_id,
        }
    }

    fn run_dir(&self) -> PathBuf {
        runs_root(&self.home).join(&self.run_id)
    }

    fn lock(&self) -> PathBuf {
        self.run_dir().join("scratch").join("engine.lock")
    }

    /// How many answers the run's log holds.
    fn answers_on_the_log(&self) -> usize {
        Storage::open(&self.home.join("yunta.db"))
            .unwrap()
            .events_for_run(&RunId::from(self.run_id.as_str()))
            .unwrap()
            .iter()
            .filter(|event| {
                matches!(
                    event.payload(),
                    Some(EventPayload::Gates(GateEvent::Resolved(_)))
                )
            })
            .count()
    }

    /// Writes the lock of an engine still on its way out: a process that ends by
    /// itself a moment later. Started from a shell that exits at once, so nothing here
    /// keeps it as a zombie, which reads as alive after it ended.
    fn held_by_an_engine_leaving(&self) {
        let leaving = std::process::Command::new("sh")
            .args(["-c", "sleep 1 > /dev/null 2>&1 & echo $!"])
            .output()
            .unwrap();
        let pid: u32 = stdout(&leaving).trim().parse().unwrap();
        let holder = serde_json::json!({
            "schema_version": 1,
            "pid": pid,
            // Later than any process's start, so the pid reads as the one that took it.
            "started_at": "2100-01-01T00:00:00Z",
        });
        std::fs::write(self.lock(), holder.to_string()).unwrap();
    }

    /// The run's log, once its last event is written.
    fn finished_log(&self) -> String {
        let path = self.run_dir().join("events.jsonl");
        wait_for(
            || {
                std::fs::read_to_string(&path)
                    .ok()
                    .filter(|log| log.contains("\"run_finished\""))
            },
            || "the answered run never finished: its resume did not reach an engine".into(),
        )
    }
}

/// The engine that parks a run lets go moments after it writes the pause, and an answer
/// can come in those moments. The answer is on the log by the time its resume looks, so
/// the resume has to read the run as parked with an answer on it, not as moving.
///
/// Reached by: `crates/engine/src/run_hold.rs`, `crates/cli/src/commands/resume.rs`
#[test]
fn a_gate_answered_while_its_engine_leaves_goes_on() {
    let run = Parked::on_its_gate();
    run.held_by_an_engine_leaving();

    let answered = yunta_in!(
        &run.repo,
        &run.home,
        &["resolve-gate", &run.run_id, "approve"]
    );
    assert!(answered.status.success(), "{}", stderr(&answered));

    let log = run.finished_log();
    assert_eq!(log.matches("\"run_resumed\"").count(), 1, "{log}");
    assert!(log.contains("\"node_finished\""), "{log}");
}

/// A resume started beside a lock nobody can read refuses in `detached.log`, which
/// nobody reads, and an answer recorded by then would leave the run reading as moving
/// with no engine behind it. So the answer is what refuses.
///
/// Reached by: `crates/cli/src/commands/resolve_gate.rs`
#[test]
fn a_gate_answered_beside_an_unreadable_lock_records_nothing() {
    let run = Parked::on_its_gate();
    std::fs::write(run.lock(), "not a lock").unwrap();

    let refused = yunta_in!(
        &run.repo,
        &run.home,
        &["resolve-gate", &run.run_id, "approve"]
    );
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    for words in [
        "has an engine lock with no readable owner",
        "scratch/engine.lock`",
    ] {
        assert!(said.contains(words), "no `{words}` in: {said}");
    }
    assert_eq!(run.answers_on_the_log(), 0);
}
