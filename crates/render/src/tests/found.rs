//! The title and the detail of a finding: each one the engine reports reads
//! as the words its site wrote from its cause, and a session's finding reads
//! as the words it wrote.

use yunta_core::events::{AskedScope, Cleaned, DistillStep, FindingCause, FindingSays, ScopeAsker};
use yunta_core::expansion::Denial;
use yunta_core::shown::Flaw;
use yunta_core::{ArtifactRefId, NodeId, TaskId};

use crate::found::{detail, said, stated_reason, title};

/// Asserts the title and the detail that `cause` reads as.
fn reads_as(cause: &FindingCause, heading: &str, body: &str) {
    assert_eq!(title(cause), heading);
    assert_eq!(detail(cause), body);
}

/// The refusals of scope requests, a fence's breach and a review's comment.
fn scope_fence_and_review_read_as_written() {
    reads_as(
        &FindingCause::ScopeDenied {
            asker: ScopeAsker::Task("task-d".into()),
            denial: Denial::ModeDeny,
            asked: Some(AskedScope {
                paths: vec!["b.txt".into()],
                reason: Some("need an adjacent fix in b.txt".to_string()),
            }),
        },
        "scope expansion denied for task `task-d`",
        "scope_expansion mode is deny (the default) — paths asked for: b.txt; agent's stated \
         reason: need an adjacent fix in b.txt",
    );
    reads_as(
        &FindingCause::ScopeDenied {
            asker: ScopeAsker::Node("reviewer".into()),
            denial: Denial::ModeDeny,
            asked: None,
        },
        "scope expansion denied for node `reviewer`",
        "scope_expansion mode is deny (the default)",
    );
    reads_as(
        &FindingCause::FenceBreached {
            adapter: "claude-code".into(),
            paths: vec!["a.rs".into(), "b.rs".into()],
        },
        "the fence let a write through where nothing else could write",
        "fence on claude-code; 2 paths reached the diff outside it: a.rs, b.rs",
    );
    reads_as(
        &FindingCause::ReviewComment {
            author: "ana".to_string(),
            body: "rename it".to_string(),
        },
        "changes requested by ana",
        "rename it",
    );
}

/// What keeps a plan from being proven, and the artifacts this binary cannot verify.
fn unprovable_and_artifacts_read_as_written() {
    reads_as(
        &FindingCause::Unprovable {
            flaw: Flaw::HollowSpecTest {
                task: TaskId::from("T001"),
                cmd: "test -f a.txt".to_string(),
            },
        },
        "the spec's test of `T001`, `test -f a.txt`, runs none of the files the spec wrote",
        "it judges the task by tests the task writes itself — run a file the spec wrote — or \
         give the task no spec, and say why",
    );
    reads_as(
        &FindingCause::ArtifactsUnverified {
            unchecked: 2,
            verified: 3,
        },
        "the run holds artifacts this binary cannot verify",
        "left unchecked: 2 artifacts of this run, named by `artifact_written` before a run kept \
         the bytes of its artifacts under `objects/` — no object answers for such an artifact, \
         and none ever did; verified: 3 artifacts the log accepted",
    );
}

#[test]
fn every_finding_the_engine_reports_reads_as_it_did() {
    scope_fence_and_review_read_as_written();
    unprovable_and_artifacts_read_as_written();
}

#[test]
fn the_engine_reports_about_the_process_and_the_cleanup_read_as_they_did() {
    reads_as(
        &FindingCause::RegistryUnwritten {
            report: "busy".to_string(),
        },
        "the process registry could not be written",
        "nothing outside this invocation can see its process tree: busy",
    );
    reads_as(
        &FindingCause::CleanupSkipped,
        "on_finish.cleanup: worktree skipped",
        "the run's tree is not a linked git worktree, so removing it would delete a primary \
         checkout — nothing was touched",
    );
    reads_as(
        &FindingCause::CleanupFailed {
            what: Cleaned::Worktree,
            report: "busy".to_string(),
        },
        "on_finish.cleanup: worktree failed",
        "the run's linked worktree could not be removed: busy",
    );
    reads_as(
        &FindingCause::CleanupFailed {
            what: Cleaned::Branch,
            report: "busy".to_string(),
        },
        "on_finish.cleanup: worktree failed",
        "the run's branch could not be deleted: busy",
    );
}

#[test]
fn a_distill_step_git_refused_reads_as_the_command_it_ran() {
    reads_as(
        &FindingCause::DistillFailed {
            step: DistillStep::Add,
        },
        "distill: `git add` failed",
        "the distilled files stay uncommitted in the run's worktree",
    );
    reads_as(
        &FindingCause::DistillFailed {
            step: DistillStep::Commit,
        },
        "distill: `git commit` failed",
        "the distilled files stay uncommitted in the run's worktree",
    );
    reads_as(
        &FindingCause::DistillFailed {
            step: DistillStep::Push,
        },
        "distill: `git push` failed",
        "the distill commit stays on the run's local branch",
    );
}

#[test]
fn a_declared_artifact_that_was_never_produced_reads_as_its_declaration() {
    reads_as(
        &FindingCause::DistillMissing {
            node: NodeId::from("grill"),
            artifact: ArtifactRefId::Name {
                name: "brief.md".into(),
            },
        },
        "distill: declared artifact `brief.md` of node `grill` was never produced",
        "the workflow's `on_finish.distill` names this artifact but the node that produces it \
         never did in this run",
    );
}

#[test]
fn an_owed_request_without_a_reason_says_a_red_criterion_located_the_writes() {
    let owed = FindingCause::ScopeDenied {
        asker: ScopeAsker::Task("task-d".into()),
        denial: Denial::ModeDeny,
        asked: Some(AskedScope {
            paths: vec!["b.txt".into()],
            reason: None,
        }),
    };
    assert_eq!(
        detail(&owed),
        "scope_expansion mode is deny (the default) — paths asked for: b.txt; agent's stated \
         reason: a red criterion of the task locates the refused writes"
    );
    assert_eq!(
        stated_reason(None),
        "a red criterion of the task locates the refused writes"
    );
    assert_eq!(stated_reason(Some("need b")), "need b");
}

#[test]
fn a_finding_a_session_wrote_reads_as_the_words_it_wrote() {
    let written = FindingSays::Written {
        title: "a title".to_string(),
        detail: "a detail".to_string(),
    };
    assert_eq!(
        said(&written),
        ("a title".to_string(), "a detail".to_string())
    );
    let found = FindingSays::Found {
        cause: FindingCause::CleanupSkipped,
    };
    assert_eq!(
        said(&found),
        (
            "on_finish.cleanup: worktree skipped".to_string(),
            "the run's tree is not a linked git worktree, so removing it would delete a primary \
             checkout — nothing was touched"
                .to_string(),
        )
    );
}
