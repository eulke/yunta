//! Scope expansion: the agent proposes, the engine disposes. Between
//! "esto no me corresponde" (a finding) and "esto excede el modo" (a
//! promotion), a task may need a small, adjacent fix outside its own
//! declared scope — it never widens its own scope by itself; it
//! requests, and this module decides.
//!
//! The model is fixed (three modes, a single request object, the
//! engine's own pre-check of the proposed criterion, a per-run cap
//! whose exhaustion escalates), and rests on two mechanics:
//!
//! - **Transport**: how the agent's request reaches the engine at all.
//!   No MCP tool surface exists anywhere in this codebase yet (`skills`/
//!   `mcp_servers` config is out of scope). This module reuses the exact
//!   pattern already established for `kind: findings`: a
//!   structured file the agent writes, the engine reads once the session
//!   ends — here, a single well-known path inside the task's own
//!   isolated worktree (`SCOPE_EXPANSION_REQUEST_FILE`), since a request
//!   is one object per task attempt, not a list.
//! - **"Tamaño acotado"** (no number given in the wording it comes from):
//!   bounded by file count rather than changed lines — simpler and
//!   robust across a mix of tracked and untracked files, still faithful
//!   to "un arreglo chico, adyacente". See [`MAX_EXPANSION_FILES`].
//!
//! A third boundary, a consequence of how pausing is wired
//! (`loop_exec.rs`): a task whose own criteria succeed on its *declared*
//! scope alone, despite leaving an `Escalate`d request behind (`ask`
//! mode, or an exhausted `max_per_run`), still pauses the run once — but
//! that pause is a one-time notification, not durable state. Nothing
//! blocks the task itself (it really did finish, on its own scope, and
//! integrates normally), so a resume after that pause does not re-pause
//! on the same unresolved request. The request stays visible forever in
//! the event log as its own audit trail regardless — a
//! `ScopeExpansionRequested` with no matching `Granted`/`Denied`
//! (Escalate has no event kind of its own) is exactly what "still owed a
//! decision" looks like on replay — but nothing here re-surfaces it
//! automatically past that first pause. Explicit, not silent: documented
//! here rather than hidden behind an unexamined resume path.

use std::path::Path;

use globset::{Glob, GlobSetBuilder};
use serde::Deserialize;
use thiserror::Error;
use yunta_core::events::{ProposedCriterion, ScopeExpansionMode};

/// The well-known path, relative to a task's own isolated worktree, an
/// agent writes to request an expansion — mirrors `findings.yaml`'s role
/// as a structured, engine-read artifact, scoped to one task
/// instead of one node since a request is task-specific (the request
/// payload is keyed by `task_id`).
pub const SCOPE_EXPANSION_REQUEST_FILE: &str = ".yunta-scope-expansion-request.yaml";

/// Documented engine convention (no number given elsewhere): at most this many
/// files may fall under one expansion request's `paths` for `rules` mode
/// to consider it "acotado".
pub const MAX_EXPANSION_FILES: usize = 5;

#[derive(Debug, Error)]
pub enum ScopeExpansionError {
    #[error("failed to read `{path}`: {detail}")]
    Read { path: String, detail: String },
    #[error("`{path}` does not parse as a scope expansion request: {detail}")]
    Malformed { path: String, detail: String },
    #[error("invalid glob `{glob}` in scope expansion request or `within`")]
    InvalidGlob {
        glob: String,
        #[source]
        source: globset::Error,
    },
    #[error("failed to {action}")]
    Io {
        action: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`git {command}` exited with status {status}: {stderr}")]
    GitFailed {
        command: String,
        status: i32,
        stderr: String,
    },
}

/// The request object required to be identical across all three
/// modes: paths, reason, and a verifiable criterion the agent proposes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
pub struct ScopeExpansionRequest {
    pub paths: Vec<String>,
    pub reason: String,
    #[serde(default)]
    pub proposed_criterion: Option<ProposedCriterion>,
}

/// Reads and parses the task's own request file, if the agent wrote one
/// this attempt. `None` — no file — is the ordinary case, not an error.
///
/// Deletes the file once read: it is a signal to the engine, not part of
/// the task's own deliverable diff, and the same worktree it lives in is
/// exactly what `scope_check` diffs against declared scope right
/// after this runs — left in place, an untracked control file would be
/// flagged a scope violation on every single request, expansion mechanics
/// aside. Consumption also gives "one request per attempt" its only real
/// enforcement: a second `load_request` against the same worktree sees
/// nothing left to read.
pub fn load_request(
    task_worktree: &Path,
) -> Result<Option<ScopeExpansionRequest>, ScopeExpansionError> {
    let path = task_worktree.join(SCOPE_EXPANSION_REQUEST_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|source| ScopeExpansionError::Read {
        path: path.display().to_string(),
        detail: source.to_string(),
    })?;
    let request = serde_yaml::from_slice(&bytes).map_err(|e| ScopeExpansionError::Malformed {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    std::fs::remove_file(&path).map_err(|source| ScopeExpansionError::Io {
        action: format!(
            "remove consumed scope expansion request `{}`",
            path.display()
        ),
        source,
    })?;
    Ok(Some(request))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Granted,
    Denied(String),
    /// `ask` mode, or the run's `max_per_run` cap already exhausted —
    /// neither is a verdict this pure evaluation renders on its own: the
    /// caller (`loop_exec`) builds the escalation object and
    /// puts it to `HumanInteraction`; only when no live surface answers
    /// does the run pause instead of guessing — degradation is always
    /// explicit, never silent.
    Escalate,
}

/// One attempt's whole expansion story — the request it found, the
/// proposed criterion's own pre-check result, and what got decided.
/// Carried on `AttemptRecord` so the caller can emit every event this
/// requires and convert a denial into a finding without re-deriving
/// any of it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeExpansionOutcome {
    pub request: ScopeExpansionRequest,
    pub precheck_exit: Option<i32>,
    pub decision: Decision,
}

/// The run's expansion-grant accounting under concurrency: the
/// expensive evaluation (proposed-criterion pre-check, rule matching,
/// diffs) runs fully concurrent outside any lock; only the cap window —
/// read the count, decide against `max_per_run`, commit the grant — is
/// atomic, so `max_per_run` holds *exactly* even when a whole batch
/// requests at once. One ledger per batch, seeded from the log (grants
/// from prior batches are already events by the time a batch starts —
/// integration is serial and completes first).
pub struct GrantLedger {
    granted: tokio::sync::Mutex<u32>,
}

impl GrantLedger {
    pub fn new(granted_so_far: u32) -> Self {
        Self {
            granted: tokio::sync::Mutex::new(granted_so_far),
        }
    }

    /// The atomic window: a provisional `Granted` commits (or turns
    /// into `Escalate` if the cap is already spent); any other decision
    /// passes through untouched — but an exhausted cap escalates
    /// regardless of what the mode would have said, same precedence a
    /// simple sequential check would apply.
    async fn commit(&self, cap: Option<u32>, provisional: Decision) -> Decision {
        let mut granted = self.granted.lock().await;
        if let Some(cap) = cap {
            if *granted >= cap {
                return Decision::Escalate;
            }
        }
        if provisional == Decision::Granted {
            *granted += 1;
        }
        provisional
    }
}

/// Decides one request: the proposed criterion's own pre-check
/// runs first, in every mode — a criterion that already passes is
/// trivial and rejected without consulting anyone, the same "pre-check
/// en rojo" logic already applies to task criteria. Then the mode
/// evaluates (still outside any lock), and the cap's atomic window
/// (`GrantLedger::commit`) has the last word.
pub async fn evaluate(
    mode: ScopeExpansionMode,
    within: &[String],
    max_per_run: Option<u32>,
    grants: &GrantLedger,
    request: &ScopeExpansionRequest,
    task_worktree: &Path,
) -> Result<(Option<i32>, Decision), ScopeExpansionError> {
    let precheck_exit = match &request.proposed_criterion {
        Some(criterion) => Some(run_criterion(task_worktree, &criterion.cmd).await?),
        None => None,
    };
    if precheck_exit == Some(0) {
        return Ok((
            precheck_exit,
            Decision::Denied(
                "proposed criterion already passes — nothing to expand for".to_string(),
            ),
        ));
    }

    let provisional = match mode {
        ScopeExpansionMode::Deny => {
            Decision::Denied("scope_expansion mode is deny (the default)".to_string())
        }
        ScopeExpansionMode::Ask => Decision::Escalate,
        ScopeExpansionMode::Rules => evaluate_rules(within, request, task_worktree).await?,
    };
    Ok((precheck_exit, grants.commit(max_per_run, provisional).await))
}

async fn evaluate_rules(
    within: &[String],
    request: &ScopeExpansionRequest,
    task_worktree: &Path,
) -> Result<Decision, ScopeExpansionError> {
    let ceiling = build_globset(within)?;
    let requested = build_globset(&request.paths)?;

    if !request
        .paths
        .iter()
        .all(|path| ceiling.is_match(Path::new(path)))
    {
        return Ok(Decision::Denied(format!(
            "requested path(s) fall outside the declared `within` ceiling: {:?}",
            request.paths
        )));
    }

    if request.proposed_criterion.is_none() {
        return Ok(Decision::Denied(
            "rules mode requires a proposed_criterion".to_string(),
        ));
    }

    let touched = diff_paths(task_worktree).await?;
    let matched: Vec<_> = touched
        .iter()
        .filter(|path| requested.is_match(path))
        .collect();
    if matched.len() > MAX_EXPANSION_FILES {
        return Ok(Decision::Denied(format!(
            "diff at the requested paths touches {} file(s), over the {MAX_EXPANSION_FILES}-file bound",
            matched.len()
        )));
    }

    Ok(Decision::Granted)
}

fn build_globset(patterns: &[String]) -> Result<globset::GlobSet, ScopeExpansionError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|source| ScopeExpansionError::InvalidGlob {
            glob: pattern.clone(),
            source,
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|source| ScopeExpansionError::InvalidGlob {
            glob: patterns.join(", "),
            source,
        })
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<String, ScopeExpansionError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|source| ScopeExpansionError::Io {
            action: format!("run `git {}`", args.join(" ")),
            source,
        })?;
    if !output.status.success() {
        return Err(ScopeExpansionError::GitFailed {
            command: args.join(" "),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn diff_paths(cwd: &Path) -> Result<Vec<std::path::PathBuf>, ScopeExpansionError> {
    let mut paths: Vec<std::path::PathBuf> = run_git(cwd, &["diff", "--name-only", "HEAD"])
        .await?
        .lines()
        .map(std::path::PathBuf::from)
        .collect();
    paths.extend(
        run_git(cwd, &["ls-files", "--others", "--exclude-standard"])
            .await?
            .lines()
            .map(std::path::PathBuf::from),
    );
    paths.sort();
    paths.dedup();
    Ok(paths)
}

async fn run_criterion(cwd: &Path, cmd: &str) -> Result<i32, ScopeExpansionError> {
    let status = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .status()
        .await
        .map_err(|source| ScopeExpansionError::Io {
            action: format!("run proposed criterion `{cmd}`"),
            source,
        })?;
    Ok(status.code().unwrap_or(-1))
}
