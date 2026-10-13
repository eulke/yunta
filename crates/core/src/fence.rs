//! The fence: which of a session's file-tool writes its scope admits,
//! judged by one pure function against the session's confinement.
//!
//! A node declares a scope and a run grants expansions; the fence holds
//! that scope as one value. What a session may write beyond the scope is
//! the [`Confinement`]'s: its roots, and the paths no session writes.
//! [`Fence::judge`] is the only thing that decides whether a path is inside
//! both, and the engine asks it for every write a session's hook reports,
//! whichever adapter opened the session. Nothing here reads the disk: a
//! judgement about a path is about the path.
//!
//! The post-check diff remains the guarantee. The fence is what keeps a
//! write outside the scope from happening at all, and the coverage the
//! confinement reports is what says how much of that the sandbox could do.

use std::path::{Component, Path, PathBuf};

use crate::confinement::Confinement;
use crate::glob::{listed_globs, scope_globset, ScopeGlob};
use crate::port::PermissionProfile;
use crate::protected::{is_register, Protected, Protection};
use serde::{Deserialize, Serialize};

/// Every refusal starts with this, so whoever reads one — the model, a
/// person reading its transcript — knows it was the fence.
pub const REFUSAL_MARKER: &str = "yunta: write refused: ";

/// What a session's file tools may write under the worktree: the globs
/// its scope admits, and what a refusal advises.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fence {
    /// What may be written under the worktree. `None` is no ceiling at
    /// all — every path under it — and is a different fact from
    /// `Some([])`, which is a ceiling that admits nothing: the read-only
    /// profile's.
    pub allowed: Option<Vec<ScopeGlob>>,
    pub advice: Advice,
}

/// What a refusal tells the model to do instead. A session that mounted
/// the scope-expansion tool can ask for more; one that did not reports
/// the need and moves on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Advice {
    RequestExpansion,
    ReportFinding,
}

/// What a judgement answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    Refused(Refusal),
}

/// A write that will not happen, and everything a reader — the model
/// first — needs to know why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub target: PathBuf,
    pub worktree: PathBuf,
    /// The ceiling the write broke: the scope's, or none under a worktree
    /// the confinement does not write. `None` never refuses under the
    /// worktree, so a refusal with `None` here is one for a path outside it.
    pub allowed: Option<Vec<ScopeGlob>>,
    pub roots: Vec<PathBuf>,
    pub advice: Advice,
    /// The protection that refuses the path, which no request widens
    /// unless it is an instruction file a person grants.
    pub protection: Option<Protection>,
}

/// How much of a session the fence actually covered, derived from what
/// the adapter could build — never declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "coverage", rename_all = "snake_case")]
pub enum Coverage {
    Exact,
    WidenedToRoots { roots: Vec<PathBuf> },
    ToolsOnly,
}

/// What one channel — the CLI's file tools, or everything else — ended
/// up fenced by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fenced {
    Exact,
    Roots(Vec<PathBuf>),
}

impl Fence {
    /// Everything under the worktree: what a session whose node declared
    /// no scope may write.
    pub fn everything(advice: Advice) -> Self {
        Fence {
            allowed: None,
            advice,
        }
    }

    /// The fence one session runs behind: its profile, the scope it
    /// declared and the expansions already granted to it. A read-only
    /// profile admits nothing under the worktree.
    pub fn for_session(
        profile: PermissionProfile,
        scope: Option<&[ScopeGlob]>,
        granted: &[ScopeGlob],
        advice: Advice,
    ) -> Self {
        match (profile, scope) {
            (PermissionProfile::ReadOnly, _) => Fence {
                allowed: Some(Vec::new()),
                advice,
            },
            (_, None) => Fence::everything(advice),
            (_, Some(scope)) => Fence {
                allowed: Some(scope.iter().chain(granted).cloned().collect()),
                advice,
            },
        }
    }

    /// Whether `target` is inside the fence and the confinement. Pure:
    /// `target` is resolved lexically against `worktree` and nothing is
    /// read from disk, so the answer is the same wherever it is asked.
    pub fn judge(&self, worktree: &Path, target: &Path, confinement: &Confinement) -> Verdict {
        let target = lexical_absolute(worktree, target);
        let inside = target.strip_prefix(worktree).ok();
        let protection =
            inside.and_then(|relative| protection_of(&confinement.protected, relative));
        let allowed = protection.is_none()
            && (confinement
                .roots
                .iter()
                .any(|root| target.starts_with(root))
                || self.admits(confinement, inside));
        if allowed {
            return Verdict::Allowed;
        }
        Verdict::Refused(Refusal {
            target,
            worktree: worktree.to_path_buf(),
            allowed: self.worktree_ceiling(confinement),
            roots: confinement.roots.clone(),
            advice: self.advice,
            protection,
        })
    }

    /// Whether the scope admits a path no protection holds. Under the
    /// worktree that takes a confinement which writes it, and a scope that
    /// names it; outside the worktree nothing is admitted here.
    fn admits(&self, confinement: &Confinement, inside: Option<&Path>) -> bool {
        match inside {
            None => false,
            Some(_) if !confinement.writes_worktree() => false,
            Some(_) if self.allowed.is_none() => true,
            Some(relative) => self
                .allowed
                .as_deref()
                .and_then(|globs| scope_globset(globs).ok())
                .is_some_and(|set| set.is_match(relative)),
        }
    }

    /// The ceiling a refusal reports: the scope's, or nothing when the
    /// confinement writes no worktree.
    fn worktree_ceiling(&self, confinement: &Confinement) -> Option<Vec<ScopeGlob>> {
        if confinement.writes_worktree() {
            self.allowed.clone()
        } else {
            Some(Vec::new())
        }
    }
}

/// The reason a worktree-relative path is refused whatever the scope
/// admits: the set's reason, else the run's own register. The register
/// holds in every confinement, even one built without the set.
fn protection_of(protected: &Protected, relative: &Path) -> Option<Protection> {
    protected
        .reason_of(relative)
        .or_else(|| is_register(relative).then_some(Protection::GitRegister))
}

impl Coverage {
    /// The weakest of the two channels. `Exact` only when the file tools
    /// and everything else are both exact; a channel nothing fenced at
    /// all makes the whole session `ToolsOnly`.
    pub fn of(tools: Fenced, others: Option<Fenced>) -> Coverage {
        let Some(others) = others else {
            return Coverage::ToolsOnly;
        };
        match (tools, others) {
            (Fenced::Exact, Fenced::Exact) => Coverage::Exact,
            (tools, others) => Coverage::WidenedToRoots {
                roots: merged_roots(tools, others),
            },
        }
    }
}

/// Both channels' roots, in order and without repeats.
fn merged_roots(tools: Fenced, others: Fenced) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for fenced in [tools, others] {
        if let Fenced::Roots(each) = fenced {
            for root in each {
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
        }
    }
    roots
}

impl Refusal {
    /// The one text of a refusal, read by the model that tried the write.
    /// `naming` is how the session's CLI calls the run's tools.
    pub fn message(&self, naming: crate::ToolNaming) -> String {
        if let Some(protection) = self.protection {
            return format!(
                "{REFUSAL_MARKER}{} is outside what this session may ever write. {} Do not write \
                 here.",
                self.target.display(),
                protection_sentence(protection)
            );
        }
        format!(
            "{REFUSAL_MARKER}{} is outside this session's scope.\n\
             Allowed under {}: {}; also writable: {}.\n{}",
            self.target.display(),
            self.worktree.display(),
            match &self.allowed {
                None => "everything".to_string(),
                Some(globs) => or_none(listed_globs(globs)),
            },
            or_none(
                self.roots
                    .iter()
                    .map(|root| root.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            self.advice_sentence(naming)
        )
    }

    /// What the refusal tells the model to do instead.
    fn advice_sentence(&self, naming: crate::ToolNaming) -> String {
        match self.advice {
            Advice::RequestExpansion => format!(
                "Ask for more scope with the run tool {}; a granted expansion applies from \
                 the next attempt. When one of your task's own criteria fails pointing at \
                 this file as `path:line`, the engine grants it itself once the attempt \
                 ends. Do not write here.",
                crate::RunTool::RequestScopeExpansion.called(naming)
            ),
            Advice::ReportFinding => "Report the need as a finding; do not write here.".to_string(),
        }
    }
}

fn or_none(list: String) -> String {
    if list.is_empty() {
        "none".to_string()
    } else {
        list
    }
}

/// The one sentence a refusal gives for the reason a path is protected.
fn protection_sentence(protection: Protection) -> &'static str {
    match protection {
        Protection::ProjectDeny => {
            "The project denies it to every run (permissions.paths.deny), \
             and no request widens that."
        }
        Protection::AgentConfig => {
            "It is the agent CLI's own configuration, which no session writes, \
             and no request widens that."
        }
        Protection::AgentInstructions => "It is an instruction file: only a person may grant it.",
        Protection::ApprovedTest => {
            "It holds a test a person approved, and no request widens that."
        }
        Protection::GitRegister => {
            "It is the run's own git register, which no session writes, \
             and no request widens that."
        }
    }
}

/// `path` as an absolute path, resolved against `base` when relative,
/// with `.` and `..` collapsed. Never reads the disk, so a path that
/// does not exist resolves exactly like one that does — and a symlink
/// is not followed, which is why the post-check diff is still the
/// guarantee.
pub fn lexical_absolute(base: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let mut resolved = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }
    resolved
}
