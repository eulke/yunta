//! The title and the detail of a finding the engine reported, written from
//! its cause; a session's finding is its own words, as it wrote them.

use std::path::PathBuf;

use yunta_core::events::{AskedScope, Cleaned, DistillStep, FindingCause, FindingSays, ScopeAsker};
use yunta_core::expansion::Denial;
use yunta_core::{listed_globs, text, AdapterId, OBJECTS_DIR};

/// The title and the detail a reader meets a finding with.
pub fn said(says: &FindingSays) -> (String, String) {
    match says {
        FindingSays::Written { title, detail } => (title.clone(), detail.clone()),
        FindingSays::Found { cause } => (title(cause), detail(cause)),
    }
}

/// The title of a finding the engine reported, in the words its site wrote.
pub fn title(cause: &FindingCause) -> String {
    match cause {
        FindingCause::ScopeDenied { asker, .. } => {
            format!("scope expansion denied for {}", asker_named(asker))
        }
        FindingCause::FenceBreached { .. } => {
            "the fence let a write through where nothing else could write".to_string()
        }
        FindingCause::ReviewComment { author, .. } => format!("changes requested by {author}"),
        FindingCause::Unprovable { flaw } => flaw.to_string(),
        FindingCause::ArtifactsUnverified { .. } => {
            "the run holds artifacts this binary cannot verify".to_string()
        }
        FindingCause::RegistryUnwritten { .. } => {
            "the process registry could not be written".to_string()
        }
        FindingCause::CleanupSkipped => "on_finish.cleanup: worktree skipped".to_string(),
        FindingCause::CleanupFailed { .. } => "on_finish.cleanup: worktree failed".to_string(),
        FindingCause::DistillFailed { step } => {
            format!("distill: `git {}` failed", step_command(*step))
        }
        FindingCause::DistillMissing { node, artifact } => {
            format!("distill: declared artifact `{artifact}` of node `{node}` was never produced")
        }
    }
}

/// The detail of a finding the engine reported, in the words its site wrote.
pub fn detail(cause: &FindingCause) -> String {
    match cause {
        FindingCause::ScopeDenied { denial, asked, .. } => scope_detail(denial, asked.as_ref()),
        FindingCause::FenceBreached { adapter, paths } => fence_detail(adapter, paths),
        FindingCause::ReviewComment { body, .. } => body.clone(),
        FindingCause::Unprovable { flaw } => format!("{} — {}", flaw.so(), flaw.fix()),
        FindingCause::ArtifactsUnverified {
            unchecked,
            verified,
        } => unverified_detail(*unchecked, *verified),
        FindingCause::RegistryUnwritten { report } => {
            format!("nothing outside this invocation can see its process tree: {report}")
        }
        FindingCause::CleanupSkipped => "the run's tree is not a linked git worktree, so \
             removing it would delete a primary checkout — nothing was touched"
            .to_string(),
        FindingCause::CleanupFailed { what, report } => cleanup_detail(*what, report),
        FindingCause::DistillFailed { step } => distill_detail(*step),
        FindingCause::DistillMissing { .. } => "the workflow's `on_finish.distill` names this \
             artifact but the node that produces it never did in this run"
            .to_string(),
    }
}

/// A scope request's reason as a reader reads it: the session's words, or,
/// when a red criterion located the refused writes, the sentence that says so.
pub fn stated_reason(reason: Option<&str>) -> &str {
    reason.unwrap_or("a red criterion of the task locates the refused writes")
}

/// The asker as a title names it: the task or the node, by its id.
fn asker_named(asker: &ScopeAsker) -> String {
    match asker {
        ScopeAsker::Task(task) => format!("task `{task}`"),
        ScopeAsker::Node(node) => format!("node `{node}`"),
    }
}

/// Why a scope request was refused, and what it asked for when the session asked.
fn scope_detail(denial: &Denial, asked: Option<&AskedScope>) -> String {
    let words = crate::shown::denial(denial);
    match asked {
        Some(asked) => format!(
            "{words} — paths asked for: {}; agent's stated reason: {}",
            listed_globs(&asked.paths),
            stated_reason(asked.reason.as_deref())
        ),
        None => words,
    }
}

/// The paths that reached the diff outside a fence, and how many there are.
fn fence_detail(adapter: &AdapterId, paths: &[PathBuf]) -> String {
    let listed: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    format!(
        "fence on {adapter}; {} paths reached the diff outside it: {}",
        paths.len(),
        listed.join(", ")
    )
}

/// The artifacts the run holds that this binary cannot verify, and those it verified.
fn unverified_detail(unchecked: usize, verified: usize) -> String {
    format!(
        "left unchecked: {} of this run, named by `artifact_written` before a run kept the \
         bytes of its artifacts under `{OBJECTS_DIR}/` — no object answers for such an \
         artifact, and none ever did; verified: {} the log accepted",
        text::counted(unchecked, "artifact"),
        text::counted(verified, "artifact"),
    )
}

/// What `on_finish.cleanup` could not take away, and what git reported.
fn cleanup_detail(what: Cleaned, report: &str) -> String {
    match what {
        Cleaned::Worktree => format!("the run's linked worktree could not be removed: {report}"),
        Cleaned::Branch => format!("the run's branch could not be deleted: {report}"),
    }
}

/// Where a distill step git refused leaves the distilled files.
fn distill_detail(step: DistillStep) -> String {
    match step {
        DistillStep::Add | DistillStep::Commit => {
            "the distilled files stay uncommitted in the run's worktree".to_string()
        }
        DistillStep::Push => "the distill commit stays on the run's local branch".to_string(),
    }
}

/// The git command a distill step runs, as its finding names it.
fn step_command(step: DistillStep) -> &'static str {
    match step {
        DistillStep::Add => "add",
        DistillStep::Commit => "commit",
        DistillStep::Push => "push",
    }
}
