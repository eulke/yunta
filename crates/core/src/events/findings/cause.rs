//! What a finding says, and the key two findings at one place share when
//! they are one complaint. A session's finding says its own words; an
//! engine's says what it found, as data that its title and detail are
//! written from wherever a reader meets them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::payloads::Finding;
use crate::expansion::Denial;
use crate::shown::Flaw;
use crate::{AdapterId, ArtifactRefId, NodeId, ScopeGlob, TaskId};

/// What a finding says: what a session wrote, or what the engine found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingSays {
    /// A session's own words; also how an older log's engine finding reads.
    Written { title: String, detail: String },
    /// What the engine found, as data.
    Found { cause: FindingCause },
}

/// What the engine found: the data its title and detail are written from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingCause {
    /// A request for scope a rule or a person refused: who asked, why it was
    /// refused, and what was asked, when the session asked.
    ScopeDenied {
        asker: ScopeAsker,
        denial: Denial,
        asked: Option<AskedScope>,
    },
    /// Writes the fence should have stopped reached the diff.
    FenceBreached {
        adapter: AdapterId,
        paths: Vec<PathBuf>,
    },
    /// One comment of a review that requested changes, in its author's words.
    ReviewComment { author: String, body: String },
    /// What keeps the plan from being proven as it is written.
    Unprovable { flaw: Flaw },
    /// Artifacts this binary cannot verify, and those it verified.
    ArtifactsUnverified { unchecked: usize, verified: usize },
    /// The process registry this invocation could not write, and what the
    /// write reported.
    RegistryUnwritten { report: String },
    /// `on_finish.cleanup` left the run's tree alone: it is not a linked
    /// worktree.
    CleanupSkipped,
    /// `on_finish.cleanup` could not take the worktree or the branch away,
    /// and what git reported.
    CleanupFailed { what: Cleaned, report: String },
    /// A step of `on_finish.distill` git refused.
    DistillFailed { step: DistillStep },
    /// An artifact `on_finish.distill` names that no node produced in this
    /// run.
    DistillMissing {
        node: NodeId,
        artifact: ArtifactRefId,
    },
}

/// Who asked for the scope a denial refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScopeAsker {
    Task(TaskId),
    Node(NodeId),
}

/// What a session asked for: the paths, and its own reason, absent when a
/// red criterion located the refused writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AskedScope {
    pub paths: Vec<ScopeGlob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// What `on_finish.cleanup` could not take away: the run's worktree, or the
/// branch it ran on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cleaned {
    Worktree,
    Branch,
}

/// The step of `on_finish.distill` that git refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DistillStep {
    Add,
    Commit,
    Push,
}

/// What makes two findings at one place one complaint, as a key: a
/// session's title, or the data an engine finding's title is written from,
/// each lowercased with its whitespace collapsed, as titles compare today.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Complaint {
    /// A session's title, normalized.
    Titled(String),
    /// An engine finding: the name of its cause, and the normalized data its
    /// title is written from.
    Found { cause: &'static str, about: String },
}

impl Finding {
    /// The key two findings at one place share when they are one complaint.
    pub fn complaint(&self) -> Complaint {
        match &self.says {
            FindingSays::Written { title, .. } => Complaint::Titled(normalized(title)),
            FindingSays::Found { cause } => cause.complaint(),
        }
    }
}

impl FindingCause {
    /// The cause's name, and the data its title is written from.
    fn complaint(&self) -> Complaint {
        let (cause, about) = match self {
            FindingCause::ScopeDenied { asker, .. } => ("scope_denied", asker.named()),
            FindingCause::ReviewComment { author, .. } => ("review_comment", author.clone()),
            FindingCause::Unprovable { flaw } => ("unprovable", flaw.to_string()),
            FindingCause::DistillFailed { step } => ("distill_failed", step.command().to_string()),
            FindingCause::DistillMissing { node, artifact } => {
                ("distill_missing", format!("{artifact} of node {node}"))
            }
            FindingCause::FenceBreached { .. } => ("fence_breached", String::new()),
            FindingCause::ArtifactsUnverified { .. } => ("artifacts_unverified", String::new()),
            FindingCause::RegistryUnwritten { .. } => ("registry_unwritten", String::new()),
            FindingCause::CleanupSkipped => ("cleanup_skipped", String::new()),
            FindingCause::CleanupFailed { .. } => ("cleanup_failed", String::new()),
        };
        Complaint::Found {
            cause,
            about: normalized(&about),
        }
    }
}

impl ScopeAsker {
    /// The asker as its complaint names it: `task T001`, or `node reviewer`.
    fn named(&self) -> String {
        match self {
            ScopeAsker::Task(task) => format!("task {task}"),
            ScopeAsker::Node(node) => format!("node {node}"),
        }
    }
}

impl DistillStep {
    /// The git command the step runs, as its complaint names it.
    fn command(self) -> &'static str {
        match self {
            DistillStep::Add => "add",
            DistillStep::Commit => "commit",
            DistillStep::Push => "push",
        }
    }
}

/// A title as two findings compare it: its words, one space apart, in
/// lowercase.
fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::FindingSeverity;
    use crate::expansion::Denial;

    fn finding(says: FindingSays) -> Finding {
        Finding {
            id: "f-1".into(),
            severity: FindingSeverity::Minor,
            says,
            location: "src/a.rs:1".into(),
            proposed_criterion: None,
        }
    }

    fn refused(asker: ScopeAsker) -> Complaint {
        finding(FindingSays::Found {
            cause: FindingCause::ScopeDenied {
                asker,
                denial: Denial::ModeDeny,
                asked: None,
            },
        })
        .complaint()
    }

    fn written(title: &str) -> Complaint {
        finding(FindingSays::Written {
            title: title.to_string(),
            detail: "d".to_string(),
        })
        .complaint()
    }

    /// A task's refusals are one complaint whatever the case of its name,
    /// because the titles they are written with compare that way.
    #[test]
    fn a_tasks_refusals_are_one_complaint_whatever_the_case_of_its_name() {
        let task = |name: &str| ScopeAsker::Task(name.into());
        assert_eq!(refused(task("T001")), refused(task("t001")));
        assert_ne!(refused(task("T001")), refused(task("T002")));
    }

    /// A node and a task that share a name are different askers.
    #[test]
    fn a_node_and_a_task_with_one_name_are_different_complaints() {
        assert_ne!(
            refused(ScopeAsker::Task("T001".into())),
            refused(ScopeAsker::Node("T001".into()))
        );
    }

    /// A written title compares with its whitespace collapsed and its letters
    /// lowercased, and never equals the complaint of an engine finding.
    #[test]
    fn a_written_title_compares_lowercased_with_its_whitespace_collapsed() {
        assert_eq!(
            written("Scope  EXPANSION\tdenied"),
            written("scope expansion denied")
        );
        assert_ne!(
            written("scope expansion denied"),
            refused(ScopeAsker::Task("T001".into()))
        );
    }

    /// Cleanup that could not take the worktree or the branch away is one
    /// complaint, as the two titles it has are one title.
    #[test]
    fn a_failed_cleanup_is_one_complaint_for_either_thing_it_could_not_take() {
        let failed = |what: Cleaned| {
            finding(FindingSays::Found {
                cause: FindingCause::CleanupFailed {
                    what,
                    report: "busy".to_string(),
                },
            })
            .complaint()
        };
        assert_eq!(failed(Cleaned::Worktree), failed(Cleaned::Branch));
    }
}
