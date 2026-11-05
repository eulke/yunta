//! A run has one engine, as a person meets it: a second `yunta resume` is refused
//! and touches nothing, and an engine that died never blocks the next one.

use std::process::{Child, Stdio};

use serde_json::Value;
use yunta_core::events::{EventDraft, EventPayload, PauseReason, RunEvent, RunPausedPayload};
use yunta_core::{NodeId, RunId};
use yunta_storage::Storage;
use yunta_testkit::{full_run_id, runs_root};

use super::*;

/// A run stopped mid-node. Its only node says it started, one line per start, and
/// holds until `go` exists, or until the markers are gone with the test, so no
/// engine outlives the test that started it.
struct Holding {
    checkout: Checkout,
    markers: tempfile::TempDir,
    run_id: String,
}

/// The workflow of a [`Holding`] run, whose markers are under `markers`.
fn holding_workflow(markers: &Path) -> String {
    format!(
        "name: hold\nnodes:\n  - id: hold\n    kind: bash\n    run: \"echo in >> {0}; until [ -f {1} ] || [ ! -e {0} ]; do sleep 0.05; done\"\n",
        markers.join("started").display(),
        markers.join("go").display(),
    )
}

/// Starts `wf.yaml` detached, waits until its node is holding, and says its id.
fn started(checkout: &Checkout, markers: &Path) -> String {
    let detached = yunta_at!(checkout, &["run", "wf.yaml", "--detach"]);
    assert!(detached.status.success(), "{}", stderr(&detached));
    wait_until(
        || markers.join("started").exists(),
        || "the detached engine never reached the node".into(),
    );
    full_run_id(&checkout.home, &run_id_from(&detached))
}

/// The log of the run at `run_dir`, once its last event is written.
fn finished_log_of(run_dir: &Path) -> String {
    let path = run_dir.join("events.jsonl");
    wait_for(
        || {
            std::fs::read_to_string(&path)
                .ok()
                .filter(|log| log.contains("\"run_finished\""))
        },
        || "the run never finished".into(),
    )
}

impl Holding {
    fn started_in(config: &str) -> Self {
        let markers = tempfile::tempdir().unwrap();
        let checkout = Checkout::new()
            .config(config)
            .workflow("wf", &holding_workflow(markers.path()))
            .committed();
        let run_id = started(&checkout, markers.path());
        Holding {
            checkout,
            markers,
            run_id,
        }
    }

    /// The same node in a workflow's child: the run this holds is the child, and
    /// the id beside it is its parent's, whose engine drives them both.
    fn started_as_a_child() -> (Self, String) {
        let markers = tempfile::tempdir().unwrap();
        let checkout = Checkout::new()
            .file(
                ".yunta/workflows/inner.yaml",
                &holding_workflow(markers.path()),
            )
            .workflow(
                "wf",
                "name: outer\nnodes:\n  - { id: compose, kind: workflow, use: inner }\n",
            )
            .committed();
        let parent = started(&checkout, markers.path());
        let child = std::fs::read_dir(runs_root(&checkout.home))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .find(|name| name != &parent)
            .expect("the parent created its child before its node started");
        let held = Holding {
            checkout,
            markers,
            run_id: child,
        };
        (held, parent)
    }

    fn starts(&self) -> usize {
        std::fs::read_to_string(self.markers.path().join("started"))
            .unwrap()
            .lines()
            .count()
    }

    fn go(&self) {
        std::fs::write(self.markers.path().join("go"), "go").unwrap();
    }

    fn run_dir(&self) -> PathBuf {
        runs_root(&self.checkout.home).join(&self.run_id)
    }

    fn lock(&self) -> PathBuf {
        self.run_dir().join("scratch").join("engine.lock")
    }

    fn engine_pid(&self) -> String {
        let registry = self.run_dir().join("scratch").join("engine.json");
        let engine: Value = serde_json::from_slice(&std::fs::read(registry).unwrap()).unwrap();
        engine["engine_pid"].to_string()
    }

    /// What a person reads this run as.
    fn handle(&self) -> String {
        let id: yunta_core::RunId = self.run_id.parse().unwrap();
        id.handle().to_string()
    }

    /// Says in the engine's registry that its node asks at its terminal, which a
    /// test has no terminal to make it do. The engine writes that file again when
    /// it registers a process group, so this waits for the node's group first.
    fn asks_at_its_terminal(&self) {
        let path = self.run_dir().join("scratch").join("engine.json");
        let mut engine = wait_for(
            || {
                let engine: Value = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
                let groups = engine["process_groups"].as_array()?;
                (!groups.is_empty()).then_some(engine)
            },
            || "the engine never registered its node's process group".into(),
        );
        engine["asking"] = serde_json::json!({ "node": "hold", "since": "2026-01-01T00:00:00Z" });
        std::fs::write(&path, serde_json::to_vec(&engine).unwrap()).unwrap();
    }

    /// A `yunta resume` this test owns, with `args` after the run's id: it goes with
    /// the test whatever it is doing by then.
    fn resume(&self, args: &[&str]) -> CliChild {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
        hermetic(&mut command, &self.checkout.repo, &self.checkout.home);
        command.args(["resume", &self.run_id]).args(args);
        CliChild::spawn(command, None).unwrap()
    }

    /// A second `yunta resume`. With no guard it would drive the run until the node
    /// ends, which waits for this test, or wait for a reset no test reaches: waiting
    /// on its exit under the shared deadline makes that a failure, not a hang.
    fn second_resume(&self, args: &[&str]) -> std::process::Output {
        self.resume(args)
            .wait_with_output()
            .expect("the second resume refuses at once")
    }

    /// Stops the run and waits for its engine to let go, so what comes next meets a
    /// parked run with nobody on it.
    fn stopped(&self) {
        let cancel = yunta_at!(&self.checkout, &["cancel", &self.run_id]);
        assert!(cancel.status.success(), "{}", stderr(&cancel));
        self.lock_goes_away();
    }

    /// Says on the run's log that it is parked until a limit's reset no test reaches,
    /// the way the engine writes it.
    fn parked_on_a_far_reset(&self) {
        let storage = Storage::open(&self.checkout.home.join("yunta.db")).unwrap();
        let id = RunId::from(self.run_id.as_str());
        let last = storage
            .events_for_run(&id)
            .unwrap()
            .last()
            .expect("the run's log")
            .timestamp;
        let pause = PauseReason::AwaitingReset {
            node: NodeId::from_static("hold"),
            at: "2100-01-01T00:00:00Z".parse().unwrap(),
        };
        let draft = EventDraft::new(
            id,
            None,
            EventPayload::Run(RunEvent::Paused(RunPausedPayload { pause })),
        )
        .expect("a draft names its subject");
        storage.append_at(&draft, last).unwrap();
    }

    /// The run's log, once its last event is written.
    fn finished_log(&self) -> String {
        finished_log_of(&self.run_dir())
    }

    /// Waits for the engine to let go, which comes after the log's last event.
    fn lock_goes_away(&self) {
        wait_until(
            || !self.lock().exists(),
            || "the engine ended and left its lock".into(),
        );
    }
}

/// How many events of `kind` a log holds.
fn count(log: &str, kind: &str) -> usize {
    log.matches(&format!("\"{kind}\"")).count()
}

const POOLED: &str = "defaults:\n  isolation: worktree\n";
const IN_PLACE: &str = "defaults:\n  isolation: none\n";

/// Reached by: `crates/cli/src/commands/resume.rs`, `crates/cli/src/commands/run/attached.rs`,
/// `crates/cli/src/commands/drive.rs`
fn a_second_resume_is_refused_and_touches_nothing(config: &str) {
    let run = Holding::started_in(config);
    let pid = run.engine_pid();

    let refused = run.second_resume(&[]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    let handle = run.handle();
    for words in [
        format!("run {handle} is already moving: pid {pid} drives it"),
        format!("yunta status {handle}"),
        format!("yunta cancel {handle}"),
    ] {
        assert!(said.contains(&words), "no `{words}` in: {said}");
    }
    assert_eq!(run.engine_pid(), pid, "engine.json still names the engine");
    assert_eq!(run.starts(), 1, "the node did not start again");

    run.go();
    let log = run.finished_log();
    assert_eq!(count(&log, "run_resumed"), 0, "{log}");
    assert_eq!(count(&log, "node_started"), 1, "{log}");
    run.lock_goes_away();
}

#[test]
fn a_second_resume_on_a_run_moving_in_a_pooled_checkout_is_refused() {
    a_second_resume_is_refused_and_touches_nothing(POOLED);
}

#[test]
fn a_second_resume_on_a_run_moving_in_a_persons_checkout_is_refused() {
    a_second_resume_is_refused_and_touches_nothing(IN_PLACE);
}

/// A program copies what it reads from a refusal, so the sentence and the commands
/// name the run whole, on the one document stdout carries.
#[test]
fn a_program_reads_the_refusal_as_one_document_with_the_next_commands() {
    let run = Holding::started_in(IN_PLACE);
    let (id, pid) = (&run.run_id, run.engine_pid());

    let refused = run.second_resume(&["--json"]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    assert!(stderr(&refused).is_empty(), "{}", stderr(&refused));
    let document: Value = serde_json::from_slice(&refused.stdout).expect("one document on stdout");
    assert_eq!(
        document["error"]["said"],
        format!("run {id} is already moving: pid {pid} drives it"),
        "{document:#}"
    );
    let next: Vec<&str> = document["error"]["next"]
        .as_array()
        .expect("the commands that go on from here")
        .iter()
        .filter_map(|step| step["command"].as_str())
        .collect();
    assert_eq!(
        next,
        [format!("yunta status {id}"), format!("yunta cancel {id}")],
        "{document:#}"
    );

    run.go();
    run.lock_goes_away();
}

/// The way on from an engine asking at its terminal is answering it there: nothing
/// else moves that run, and a resume that said it was moving would hide why it waits.
#[test]
fn a_run_asking_at_its_terminal_is_said_to_be_answered_there() {
    let run = Holding::started_in(IN_PLACE);
    let pid = run.engine_pid();
    run.asks_at_its_terminal();

    let refused = run.second_resume(&[]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    let words = format!("node `hold` is asking at its terminal (pid {pid}); answer it there");
    assert!(said.contains(&words), "no `{words}` in: {said}");

    run.go();
    run.lock_goes_away();
}

/// A lock that names nobody says nothing about who drives the run, so it is never
/// taken: a person reads which file it is, and decides.
#[test]
fn a_lock_nobody_can_read_is_named_and_left_for_a_person() {
    let run = Holding::started_in(IN_PLACE);
    run.stopped();
    std::fs::write(run.lock(), "not a lock").unwrap();

    let refused = run.second_resume(&[]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    let handle = run.handle();
    for words in [
        format!("run {handle} has an engine lock with no readable owner"),
        "scratch/engine.lock`".to_string(),
        "if nothing drives this run, delete that file by hand and resume it again".to_string(),
    ] {
        assert!(said.contains(&words), "no `{words}` in: {said}");
    }
    assert_eq!(std::fs::read_to_string(run.lock()).unwrap(), "not a lock");
    assert_eq!(run.starts(), 1, "the node did not start again");
}

/// A run parked on a limit can wait for hours, and a lock nobody can check does not
/// clear by waiting: a resume that found out after the wait would have said nothing
/// until a date no test reaches.
#[test]
fn a_lock_nobody_can_check_refuses_before_a_limits_reset_is_waited_for() {
    let run = Holding::started_in(IN_PLACE);
    run.stopped();
    run.parked_on_a_far_reset();
    std::fs::write(run.lock(), "not a lock").unwrap();

    let refused = run.second_resume(&[]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    assert!(said.contains("no readable owner"), "{said}");
    assert!(said.contains("scratch/engine.lock`"), "{said}");
    assert_eq!(run.starts(), 1, "the node did not start again");
}

/// The engine that parks a run lets go moments after the pause reaches the log, and
/// answering a gate resumes the run right then: a live holder of a parked run is on
/// its way out, so it is waited for and not refused. Here the holder is a process
/// that ends by itself a moment later, and this test does not wait to find out: a
/// resume that refused is over with exit 1 before the node could start again.
#[test]
fn a_run_just_parked_waits_for_the_engine_that_parked_it() {
    let run = Holding::started_in(IN_PLACE);
    run.stopped();
    // Started from a shell that exits at once, so nothing here keeps it as a zombie,
    // which reads as alive after it ended.
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
    std::fs::write(run.lock(), holder.to_string()).unwrap();

    let resume = run.resume(&[]);
    wait_until(
        || run.starts() == 2,
        || "the resume never took over from the engine that left".into(),
    );
    run.go();
    let resumed = resume.wait_with_output().unwrap();
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    let log = run.finished_log();
    assert_eq!(count(&log, "run_resumed"), 1, "{log}");
    run.lock_goes_away();
}

/// The child has its own log and its own directory, and a person can name it: its
/// parent's engine drives it, so a second engine on it is refused like any other.
///
/// Reached by: `crates/engine/src/run/workflow_exec/child.rs`
#[test]
fn a_second_resume_on_a_child_its_parent_drives_is_refused() {
    let (child, parent) = Holding::started_as_a_child();

    let refused = child.second_resume(&[]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    assert_eq!(child.starts(), 1, "the child's node did not start again");

    child.go();
    let log = child.finished_log();
    assert_eq!(count(&log, "run_resumed"), 0, "{log}");
    assert_eq!(count(&log, "node_started"), 1, "{log}");
    child.lock_goes_away();
    finished_log_of(&runs_root(&child.checkout.home).join(parent));
}

/// The node holds until `go`, and `go` is written only after three of the resumes
/// have exited, so those three were refused while the fourth was driving: with no
/// guard, all four would drive and this would never get past the wait.
#[test]
fn of_four_resumes_at_once_one_drives_and_the_others_are_refused() {
    let run = Holding::started_in(IN_PLACE);
    let cancel = yunta_at!(&run.checkout, &["cancel", &run.run_id]);
    assert!(cancel.status.success(), "{}", stderr(&cancel));
    assert_eq!(run.starts(), 1);

    let mut resumes: Vec<Child> = (0..4)
        .map(|_| {
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
            hermetic(&mut command, &run.checkout.repo, &run.checkout.home);
            command
                .args(["resume", &run.run_id])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();

    let mut codes = Vec::new();
    for refusal in 1..=3 {
        wait_until(
            || {
                resumes.retain_mut(|resume| match resume.try_wait().unwrap() {
                    Some(status) => {
                        codes.push(status.code());
                        false
                    }
                    None => true,
                });
                codes.len() >= refusal
            },
            || format!("refusal {refusal} of 3 never came: more than one resume is driving"),
        );
    }
    assert_eq!(codes, [Some(1), Some(1), Some(1)]);
    assert_eq!(resumes.len(), 1, "the one left is the engine");
    wait_until(
        || run.starts() == 2,
        || "the engine never started the node again".into(),
    );

    run.go();
    let status = resumes.pop().unwrap().wait().unwrap();
    assert!(status.success(), "the engine ends the run: {status}");
    let log = run.finished_log();
    assert_eq!(count(&log, "run_resumed"), 1, "{log}");
    assert_eq!(run.starts(), 2, "only one engine started the node");
    run.lock_goes_away();
}

#[test]
fn an_engine_killed_mid_node_leaves_its_lock_and_the_next_resume_replaces_it() {
    let run = Holding::started_in(IN_PLACE);
    let pid = run.engine_pid();
    let signal = |args: &[&str]| {
        std::process::Command::new("kill")
            .args(args)
            .status()
            .unwrap()
    };
    assert!(signal(&["-9", &pid]).success());
    wait_until(
        || !signal(&["-0", &pid]).success(),
        || "the killed engine is still there".into(),
    );
    assert!(
        run.lock().exists(),
        "an engine that dies at once lets nothing go"
    );

    run.go();
    let resumed = yunta_at!(&run.checkout, &["resume", &run.run_id]);
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    let log = run.finished_log();
    assert_eq!(count(&log, "run_resumed"), 1, "{log}");
    run.lock_goes_away();
}
