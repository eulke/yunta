//! The fence: what a session may write, said once and judged by one
//! pure function.
//!
//! A node declares a scope; a run grants expansions; a run's own
//! directories stay writable wherever a session sits. The fence is all
//! of that as one value, and [`Fence::judge`] is the only thing that
//! decides whether a path is inside it — the engine asks it for every
//! write a session's hook reports, whichever adapter opened the session.
//! Nothing here reads the disk: a judgement about a path is about the
//! path.
//!
//! The post-check diff remains the guarantee. The fence is what keeps a
//! write outside the scope from happening at all, and the coverage it
//! reports is what says how much of that it could actually do.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::glob::{listed_globs, scope_globset, ScopeGlob};
use crate::port::PermissionProfile;
use serde::{Deserialize, Serialize};

/// Every refusal starts with this, so whoever reads one — the model, a
/// person reading its transcript — knows it was the fence.
pub const REFUSAL_MARKER: &str = "yunta: write refused: ";

/// What a session may write: the globs it may write under the worktree,
/// and the absolute roots it may write outside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fence {
    /// What may be written under the worktree. `None` is no ceiling at
    /// all — every path under it — and is a different fact from
    /// `Some([])`, which is a ceiling that admits nothing: the
    /// `read_only` profile, whose roots stay writable because they are
    /// the run's register and not the task's work.
    pub allowed: Option<Vec<ScopeGlob>>,
    pub roots: Vec<PathBuf>,
    pub advice: Advice,
    /// What no session of the run may write under the worktree — what the
    /// project denies to every run, and the tests a person approved —
    /// refused whatever `allowed` admits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied: Vec<ScopeGlob>,
    /// How the session's CLI names the run's tools, which is what a
    /// refusal that advises one names it by.
    #[serde(default)]
    pub naming: crate::ToolNaming,
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
    /// The ceiling the write broke, as the fence carries it: `None`
    /// never refuses under the worktree, so a refusal with `None` here
    /// is one for a path outside it.
    pub allowed: Option<Vec<ScopeGlob>>,
    pub roots: Vec<PathBuf>,
    pub advice: Advice,
    /// Whether the path is one no session of the run may write, which no
    /// request widens.
    pub denied: bool,
    /// How the model the refusal reaches calls the run's tools.
    pub naming: crate::ToolNaming,
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
    /// Everything under the worktree, plus the roots: what a session
    /// whose node declared no scope may write.
    pub fn everything(roots: Vec<PathBuf>, advice: Advice) -> Self {
        Fence {
            allowed: None,
            roots,
            advice,
            denied: Vec::new(),
            naming: crate::ToolNaming::default(),
        }
    }

    /// Nothing under the worktree; the roots stay writable, because they
    /// are the run's own register and not the task's work.
    pub fn read_only(roots: Vec<PathBuf>, advice: Advice) -> Self {
        Fence {
            allowed: Some(Vec::new()),
            roots,
            advice,
            denied: Vec::new(),
            naming: crate::ToolNaming::default(),
        }
    }

    /// The fence one session runs behind: its profile, the scope it
    /// declared, the expansions already granted to it, and the directory
    /// its declared files are written to.
    pub fn for_session(
        profile: PermissionProfile,
        scope: Option<&[ScopeGlob]>,
        granted: &[ScopeGlob],
        artifact_dir: Option<&Path>,
        advice: Advice,
    ) -> Self {
        let roots: Vec<PathBuf> = artifact_dir.map(Path::to_path_buf).into_iter().collect();
        match (profile, scope) {
            (PermissionProfile::ReadOnly, _) => Fence::read_only(roots, advice),
            (_, None) => Fence::everything(roots, advice),
            (_, Some(scope)) => Fence {
                allowed: Some(scope.iter().chain(granted).cloned().collect()),
                roots,
                advice,
                denied: Vec::new(),
                naming: crate::ToolNaming::default(),
            },
        }
    }

    /// The same fence, keeping `dirs` writable beside the checkout as
    /// well — the directories every session of a run shares.
    pub fn sharing(mut self, dirs: &[PathBuf]) -> Self {
        self.roots.extend(dirs.iter().cloned());
        self
    }

    /// The same fence, refusing `denied` whatever it otherwise admits.
    pub fn denying(self, denied: &[ScopeGlob]) -> Self {
        Fence {
            denied: denied.to_vec(),
            ..self
        }
    }

    /// The same fence, its refusals naming a run tool the way a CLI that
    /// names tools by `naming` hands it to its model.
    pub fn naming(self, naming: crate::ToolNaming) -> Self {
        Fence { naming, ..self }
    }

    /// Whether `target` is inside the fence. Pure: `target` is resolved
    /// lexically against `worktree` and nothing is read from disk, so
    /// the answer is the same wherever it is asked.
    pub fn judge(&self, worktree: &Path, target: &Path) -> Verdict {
        let target = lexical_absolute(worktree, target);
        if self.roots.iter().any(|root| target.starts_with(root)) {
            return Verdict::Allowed;
        }
        let inside = target.strip_prefix(worktree).ok();
        let denied = inside.is_some_and(|relative| {
            scope_globset(&self.denied).is_ok_and(|set| set.is_match(relative))
        });
        let allowed = !denied
            && match inside {
                // The run's own register is never a task's work. `.git`
                // bare is the file a `git worktree` leaves behind.
                Some(relative) if is_git(relative) => false,
                Some(_) if self.allowed.is_none() => true,
                Some(relative) => self
                    .allowed
                    .as_deref()
                    .and_then(|globs| scope_globset(globs).ok())
                    .is_some_and(|set| set.is_match(relative)),
                None => false,
            };
        if allowed {
            Verdict::Allowed
        } else {
            Verdict::Refused(Refusal {
                target,
                worktree: worktree.to_path_buf(),
                allowed: self.allowed.clone(),
                roots: self.roots.clone(),
                advice: self.advice,
                denied,
                naming: self.naming,
            })
        }
    }
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

impl fmt::Display for Refusal {
    /// The one text of a refusal, read by the model that tried the
    /// write.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denied {
            return write!(
                f,
                "{REFUSAL_MARKER}{} is outside what this session may ever write: the project \
                 denies it to every run (permissions.paths.deny), or it holds a test a person \
                 approved, and no request widens that. Do not write here.",
                self.target.display()
            );
        }
        writeln!(
            f,
            "{REFUSAL_MARKER}{} is outside this session's scope.",
            self.target.display()
        )?;
        writeln!(
            f,
            "Allowed under {}: {}; also writable: {}.",
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
            )
        )?;
        match self.advice {
            Advice::RequestExpansion => write!(
                f,
                "Ask for more scope with the run tool {}; a granted expansion applies from \
                 the next attempt. When one of your task's own criteria fails pointing at \
                 this file as `path:line`, the engine grants it itself once the attempt \
                 ends. Do not write here.",
                crate::RunTool::RequestScopeExpansion.called(self.naming)
            ),
            Advice::ReportFinding => {
                f.write_str("Report the need as a finding; do not write here.")
            }
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

/// Whether a worktree-relative path is the run's own git register.
fn is_git(relative: &Path) -> bool {
    relative
        .components()
        .next()
        .is_some_and(|first| first.as_os_str() == ".git")
}
