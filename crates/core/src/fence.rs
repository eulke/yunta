//! The fence: what a session may write, said once and judged by one
//! pure function.
//!
//! A node declares a scope; a run grants expansions; a run's own
//! directories stay writable wherever a session sits. The fence is all
//! of that as one value, and [`Fence::judge`] is the only thing that
//! decides whether a path is inside it — the CLI's hook, the mock's
//! fixture effects and every adapter's codec ask the same function.
//! Nothing here reads the disk: a judgement about a path is about the
//! path.
//!
//! The post-check diff remains the guarantee. The fence is what keeps a
//! write outside the scope from happening at all, and the coverage it
//! reports is what says how much of that it could actually do.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::glob::{listed_globs, scope_globset, ScopeGlob};
use crate::ids::AdapterId;
use crate::port::PermissionProfile;

/// The environment variable a session's fence travels in. Globs and
/// paths, never a secret: the hook reads it from the child's own
/// environment.
pub const ENV_VAR: &str = "YUNTA_FENCE";

/// The hidden subcommand that is the hook. Named here so the CLI that
/// registers it and the adapters that embed it in a CLI's config cannot
/// disagree.
pub const SUBCOMMAND: &str = "fence";

/// Every refusal starts with this, so a CLI that only gives us prose
/// still gives us something to recognise.
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

/// The hook a CLI runs before it writes: this binary, its `fence`
/// subcommand, and the adapter whose codec reads the call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FenceHook {
    bin: PathBuf,
}

/// What the fence looks like in the child's environment: the fence
/// itself plus the worktree every relative path is judged against.
#[derive(Debug, Serialize, Deserialize)]
struct FenceEnv {
    fence: Fence,
    worktree: PathBuf,
}

/// A `YUNTA_FENCE` that does not read.
#[derive(Debug, Error)]
pub enum FenceEnvError {
    #[error("`{ENV_VAR}` is not a fence")]
    Json(#[from] serde_json::Error),
}

impl Fence {
    /// Everything under the worktree, plus the roots: what a session
    /// whose node declared no scope may write.
    pub fn everything(roots: Vec<PathBuf>, advice: Advice) -> Self {
        Fence {
            allowed: None,
            roots,
            advice,
        }
    }

    /// Nothing under the worktree; the roots stay writable, because they
    /// are the run's own register and not the task's work.
    pub fn read_only(roots: Vec<PathBuf>, advice: Advice) -> Self {
        Fence {
            allowed: Some(Vec::new()),
            roots,
            advice,
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
            },
        }
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
        let allowed = match inside {
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
            })
        }
    }

    /// The fence as the child's environment carries it.
    pub fn to_env(&self, worktree: &Path) -> (&'static str, String) {
        let env = FenceEnv {
            fence: self.clone(),
            worktree: worktree.to_path_buf(),
        };
        // `Fence` and `PathBuf` both serialize; a failure here would be
        // a bug in this module, and an empty fence refuses everything,
        // which is the safe side of it.
        (ENV_VAR, serde_json::to_string(&env).unwrap_or_default())
    }

    /// The fence and the worktree a hook was handed.
    pub fn from_env(value: &str) -> Result<(Self, PathBuf), FenceEnvError> {
        let env: FenceEnv = serde_json::from_str(value)?;
        Ok((env.fence, env.worktree))
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

impl FenceHook {
    /// The binary a CLI will run. The shell resolves this once, at
    /// startup, from its own executable.
    pub fn new(bin: PathBuf) -> Self {
        FenceHook { bin }
    }

    /// The command line a CLI's config embeds, for the adapter whose
    /// codec reads what it sends.
    pub fn command(&self, adapter: &AdapterId) -> Vec<String> {
        vec![
            self.bin.display().to_string(),
            SUBCOMMAND.to_string(),
            adapter.to_string(),
        ]
    }
}

impl fmt::Display for Refusal {
    /// The one text of a refusal: read by the model that tried the
    /// write, and by [`refused_target`] reading it back out of a CLI's
    /// stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        f.write_str(match self.advice {
            Advice::RequestExpansion => {
                "Ask for more scope with the run tool yunta_request_scope_expansion; \
                 a granted expansion applies from the next attempt. Do not write here."
            }
            Advice::ReportFinding => "Report the need as a finding; do not write here.",
        })
    }
}

fn or_none(list: String) -> String {
    if list.is_empty() {
        "none".to_string()
    } else {
        list
    }
}

/// The path a refusal names, read back out of whatever a CLI passed
/// through. The one parser of [`REFUSAL_MARKER`]; text without it is not
/// a refusal.
pub fn refused_target(text: &str) -> Option<PathBuf> {
    let after = text.split_once(REFUSAL_MARKER)?.1;
    let target = after.split_once(" is outside")?.0;
    Some(PathBuf::from(target))
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
