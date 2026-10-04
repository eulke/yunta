//! What a run's log already answered. Every criterion a check, a session's
//! check, a document's hand-over or the suite's measurement recorded names
//! the tree it answered for; a wake takes those answers into its memo, so a
//! check whose insumo did not change is not run again only because the
//! invocation that ran it ended.
//!
//! An answer is taken only where it still speaks for this invocation: it
//! names its tree — and, for a command that runs `git`, the commit — it
//! answered at all, and the invocation that wrote it ran its commands with
//! the environment this one runs them with.

use yunta_core::events::{
    ArtifactEvent, BaselineOrigin, CriterionResult, EventPayload, NodeEvent, RunEvent, RunLedger,
    StoredEvent, TaskEvent,
};

use super::criteria::could_not_run;
use super::tree::{asks_git, Tree};
use crate::process::Printed;

/// One answer the log holds, as the memo keeps it.
pub(super) struct Recorded {
    pub(super) cmd: String,
    pub(super) tree: Tree,
    pub(super) exit_code: i32,
    pub(super) printed: Option<Printed>,
}

/// Every answer `events` recorded that still speaks for the invocation
/// `run` says is running now.
pub(super) fn answers(events: &[StoredEvent], run: &RunLedger) -> Vec<Recorded> {
    let Some(now) = run.environment_now() else {
        return Vec::new();
    };
    events
        .iter()
        .filter(|event| run.environment_at(event.seq) == Some(now))
        .flat_map(of_event)
        .collect()
}

/// The answers one event recorded.
fn of_event(event: &StoredEvent) -> Vec<Recorded> {
    let results: Vec<&CriterionResult> = match event.payload() {
        Some(EventPayload::Node(NodeEvent::CriteriaChecked(check))) => {
            check.results.iter().collect()
        }
        Some(EventPayload::Tasks(TaskEvent::CheckAnswered(check))) => {
            check.results.iter().collect()
        }
        Some(EventPayload::Artifacts(ArtifactEvent::Submitted(submitted))) => submitted
            .probes
            .iter()
            .flat_map(|probe| probe.results.iter())
            .collect(),
        Some(EventPayload::Run(RunEvent::BaselineCaptured(measured))) => {
            return measurement(measured).into_iter().collect();
        }
        _ => return Vec::new(),
    };
    results.into_iter().filter_map(of_result).collect()
}

/// A criterion's recorded answer, when it is one the memo can take.
fn of_result(result: &CriterionResult) -> Option<Recorded> {
    if could_not_run(result.exit_code).is_some() {
        return None;
    }
    let tree = held(&result.cmd, result.tree.clone()?, result.head.clone())?;
    Some(Recorded {
        cmd: result.cmd.clone(),
        tree,
        exit_code: result.exit_code,
        printed: (result.exit_code != 0).then(|| Printed::Recorded {
            object: result.output.clone(),
            tail: result.tail.clone(),
        }),
    })
}

/// The suite's answer on the tree this run measured it on. A red one
/// holds no task to anything and keeps nothing worth taking.
fn measurement(measured: &yunta_core::events::BaselineCapturedPayload) -> Option<Recorded> {
    if measured.origin != BaselineOrigin::Measured || !measured.passed() {
        return None;
    }
    Some(Recorded {
        cmd: measured.command.clone(),
        tree: held(&measured.command, measured.tree.clone()?, None)?,
        exit_code: 0,
        printed: None,
    })
}

/// The tree an answer of `cmd` is kept for — `None` for a command that
/// runs `git` and recorded no commit, whose answer can turn on history.
fn held(cmd: &str, content: yunta_core::TreeId, head: Option<String>) -> Option<Tree> {
    if asks_git(cmd) && head.is_none() {
        return None;
    }
    Some(Tree { content, head })
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        CriteriaCheckedPayload, EventBody, EventMeta, ExecutionEnvironment, Phase,
        RunCreatedPayload, RunResumedPayload,
    };
    use yunta_core::{RunId, Seq, TreeId};

    use super::*;

    fn environment(path: &str) -> ExecutionEnvironment {
        ExecutionEnvironment {
            shell: "/bin/sh".to_string(),
            path: vec![path.to_string()],
        }
    }

    fn stored(seq: u64, payload: EventPayload) -> StoredEvent {
        StoredEvent {
            run_id: "01M42HGDC1CHV6VQSH4MGTZMF2"
                .parse::<RunId>()
                .expect("a run id"),
            seq: Seq::from(seq),
            timestamp: chrono::DateTime::UNIX_EPOCH,
            node_id: None,
            body: EventBody::Known(payload),
        }
    }

    fn created(path: &str) -> EventPayload {
        EventPayload::Run(RunEvent::Created(RunCreatedPayload {
            manifest_hash: yunta_core::sha256_hex(b"manifest"),
            inputs: Default::default(),
            mode: Default::default(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: yunta_core::sha256_hex(b"base").as_str().into(),
            environment: Some(Box::new(environment(path))),
            left_out: Vec::new(),
        }))
    }

    fn result(
        cmd: &str,
        exit_code: i32,
        tree: Option<&str>,
        head: Option<&str>,
    ) -> CriterionResult {
        CriterionResult {
            cmd: cmd.to_string(),
            exit_code,
            r#type: None,
            reused: false,
            duration_ms: None,
            output: None,
            tail: vec!["said this".to_string()],
            tree: tree.map(|tree| tree.parse::<TreeId>().expect("a tree id")),
            head: head.map(str::to_string),
        }
    }

    fn checked(results: Vec<CriterionResult>) -> EventPayload {
        EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
            task_id: "T1".into(),
            phase: Phase::Post,
            results,
            waiting: Vec::new(),
        }))
    }

    fn ledger_of(events: &[StoredEvent]) -> RunLedger {
        let mut ledger = RunLedger::default();
        for event in events {
            if let Some(EventPayload::Run(run)) = event.payload() {
                let meta = EventMeta {
                    seq: event.seq,
                    at: event.timestamp,
                    node: None,
                };
                ledger.apply(run, &meta);
            }
        }
        ledger
    }

    const TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

    /// Only an answer the memo can take is taken: one with a tree, that
    /// answered at all, and — for a command that runs git — with the
    /// commit it stood on; a red one keeps what it said.
    #[test]
    fn only_answers_that_speak_for_a_tree_are_taken() {
        let events = vec![
            stored(1, created("/bin")),
            stored(
                2,
                checked(vec![
                    result("test -f a", 1, Some(TREE), None),
                    result("true", 0, None, None),
                    result("missing", 127, Some(TREE), None),
                    result("git log", 0, Some(TREE), None),
                    result("git status", 0, Some(TREE), Some("abc")),
                ]),
            ),
        ];

        let taken = answers(&events, &ledger_of(&events));

        let cmds: Vec<&str> = taken.iter().map(|answer| answer.cmd.as_str()).collect();
        assert_eq!(cmds, vec!["test -f a", "git status"]);
        let Some(Printed::Recorded { tail, .. }) = &taken[0].printed else {
            panic!("a red answer keeps what it said");
        };
        assert_eq!(tail, &vec!["said this".to_string()]);
    }

    /// An answer written while the commands ran with another environment
    /// is not this invocation's to take.
    #[test]
    fn answers_from_another_environment_are_left() {
        let events = vec![
            stored(1, created("/bin")),
            stored(2, checked(vec![result("test -f a", 0, Some(TREE), None)])),
            stored(
                3,
                EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(
                    Vec::new(),
                    Some(environment("/elsewhere")),
                ))),
            ),
        ];

        assert!(answers(&events, &ledger_of(&events)).is_empty());
    }
}
