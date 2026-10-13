//! Scope expansion: the agent proposes, the engine disposes. A task may
//! need a small, adjacent change outside its declared scope. It never
//! widens its own scope: it requests, and the engine decides.
//!
//! - **Transport**: the session calls `yunta_request_scope_expansion`,
//!   which writes the request to [`SCOPE_EXPANSION_REQUEST_FILE`] in the
//!   task's own worktree. The engine reads it once the session ends
//!   ([`load_request`]). A request is one object per task attempt.
//! - **Decision**: `yunta_core::expansion` decides every request, and
//!   every refused write that a red criterion locates. This module
//!   gathers the facts that decision reads and applies it through
//!   `expand`, counting the engine's grants in a [`GrantWindow`].
//! - **Bounded size**: a `rules` grant is bounded by file count rather
//!   than changed lines, so it stays a small, adjacent fix. The bound is
//!   `limits.max_expansion_files`.
//!
//! An escalated request waits for a person's answer, and the task blocks
//! until then.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;
use yunta_core::events::ToolTarget;
use yunta_core::expansion::{touched_under, Ask, Bounds, Candidate, Decision, Precheck, Screen};
use yunta_core::ProposedCriterionEntry;
use yunta_core::ScopeGlob;

use crate::git::GitError;
use crate::process::{spawn_governed, Capture, GovernedCommand, Outcome, Supervision};

/// The well-known path, relative to a task's own isolated worktree, an
/// agent writes to request an expansion — mirrors `findings.yaml`'s role
/// as a structured, engine-read artifact, scoped to one task
/// instead of one node since a request is task-specific (the request
/// payload is keyed by `task_id`).
pub const SCOPE_EXPANSION_REQUEST_FILE: &str = ".yunta-scope-expansion-request.yaml";

#[derive(Debug, Error)]
pub enum ScopeExpansionError {
    #[error("failed to read `{path}`: {detail}")]
    Read { path: String, detail: String },
    #[error("`{path}` does not parse as a scope expansion request: {detail}")]
    Malformed { path: String, detail: String },
    #[error("failed to run proposed criterion `{cmd}`")]
    Process {
        cmd: String,
        #[source]
        source: crate::process::SpawnError,
    },
    #[error("failed to {action}")]
    Io {
        action: String,
        #[source]
        source: std::io::Error,
    },
    /// git could not say what the task's tree holds: it failed, or the
    /// token stopped it.
    #[error(transparent)]
    Git(#[from] GitError),
}

impl ScopeExpansionError {
    /// Whether the token stopped the git this evaluation asked.
    pub fn cancelled(&self) -> bool {
        matches!(self, ScopeExpansionError::Git(git) if git.cancelled())
    }
}

/// The request object: paths, reason, and a verifiable criterion the agent
/// proposes or cites as its evidence.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeExpansionRequest {
    pub paths: Vec<ScopeGlob>,
    pub reason: String,
    #[serde(default)]
    pub proposed_criterion: Option<ProposedCriterionEntry>,
    /// One of the task's own criteria, red on the session's work, whose
    /// output locates every path asked for — `path:line` — in place of a
    /// proposed criterion: what lets the engine grant the request itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Reads and parses the task's own request file, if the agent wrote one
/// this attempt. `None` — no file — is the ordinary case, not an error.
///
/// Deletes the file once read: it is a signal to the engine, not part of
/// the task's own deliverable diff, and the scope audit that follows reads
/// the same worktree it lives in — left in place, an untracked control file
/// would be flagged a scope violation on every single request. Consumption
/// also gives "one request per attempt" its only real enforcement: a second
/// `load_request` against the same worktree sees nothing left to read.
pub async fn load_request(
    task_worktree: &Path,
) -> Result<Option<ScopeExpansionRequest>, ScopeExpansionError> {
    let path = task_worktree.join(SCOPE_EXPANSION_REQUEST_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|source| ScopeExpansionError::Read {
            path: path.display().to_string(),
            detail: source.to_string(),
        })?;
    let request =
        yunta_core::yaml::parse_bytes(&bytes).map_err(|e| ScopeExpansionError::Malformed {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
    tokio::fs::remove_file(&path)
        .await
        .map_err(|source| ScopeExpansionError::Io {
            action: format!(
                "remove consumed scope expansion request `{}`",
                path.display()
            ),
            source,
        })?;
    Ok(Some(request))
}

/// One attempt's expansion story: what the session asked, the pre-check of
/// its criterion, and what was decided. A located refusal has no request,
/// so `asked` is `None`. The caller turns the outcome into events and a
/// finding.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeExpansionOutcome {
    /// The request the session made, as it made it. `None` when a red
    /// output located a refused write, which the session did not ask for.
    pub asked: Option<ScopeExpansionRequest>,
    /// The pre-check of the request's own criterion, run by the caller.
    pub precheck: Option<Precheck>,
    pub decision: Decision,
    /// The engine's grants in this run once this decision is made: the
    /// count `max_per_run` holds, which a grant raises.
    pub count: u32,
}

/// The engine's grants in one run, counted by one window per batch. Each
/// request of a batch decides against the count one at a time, so
/// `max_per_run` holds exactly when several members ask at once.
pub struct GrantWindow {
    engine_grants: tokio::sync::Mutex<u32>,
}

impl GrantWindow {
    /// A window that starts from the grants the log already holds for the
    /// run, which earlier batches made.
    pub fn seeded(engine_grants: u32) -> Self {
        Self {
            engine_grants: tokio::sync::Mutex::new(engine_grants),
        }
    }

    /// The engine's grants this window counts so far.
    pub async fn granted(&self) -> u32 {
        *self.engine_grants.lock().await
    }

    /// Decides `candidate` against the count, which a grant raises. The
    /// decision comes with the count it leaves, so events number decisions
    /// in the order they were made.
    pub(crate) async fn settle(
        &self,
        candidate: Candidate,
        touched: usize,
        bounds: &Bounds<'_>,
    ) -> (Decision, u32) {
        let mut granted = self.engine_grants.lock().await;
        let decision = yunta_core::expansion::settle(candidate, touched, *granted, bounds);
        if matches!(decision, Decision::Grant { .. }) {
            *granted += 1;
        }
        (decision, *granted)
    }
}

/// Decides one request, or one located refusal. Core's `screen` settles
/// what needs no diff: protection, the pre-check, the mode and the batch.
/// A candidate it leaves open is bounded by the files this attempt touched
/// when its mode bounds it, and then settled against `window`. The caller
/// has already run any proposed criterion, which `ask` carries.
pub(crate) async fn expand(
    ask: Ask<'_>,
    bounds: &Bounds<'_>,
    window: &GrantWindow,
    worktree: &Path,
    supervision: Supervision<'_>,
) -> Result<(Decision, u32), ScopeExpansionError> {
    match yunta_core::expansion::screen(&ask, bounds) {
        Screen::Settled(decision) => Ok((decision, window.granted().await)),
        Screen::Candidate(candidate) => {
            let touched = match candidate.bounded {
                true => touched_under(&candidate, &diff_paths(worktree, supervision).await?),
                false => 0,
            };
            Ok(window.settle(candidate, touched, bounds).await)
        }
    }
}

async fn diff_paths(
    cwd: &Path,
    supervision: Supervision<'_>,
) -> Result<Vec<std::path::PathBuf>, ScopeExpansionError> {
    let mut paths: Vec<std::path::PathBuf> =
        crate::git::read(cwd, &["diff", "--name-only", "HEAD"], supervision)
            .await?
            .lines()
            .map(std::path::PathBuf::from)
            .collect();
    paths.extend(
        crate::git::read(
            cwd,
            &["ls-files", "--others", "--exclude-standard"],
            supervision,
        )
        .await?
        .lines()
        .map(std::path::PathBuf::from),
    );
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Runs a proposed criterion on `cwd` and reads its exit as a pre-check: 0
/// passes, a command the engine stopped before it answered could not run,
/// and any other exit is red. Only the exit is kept, so what the command
/// prints is dropped rather than reaching the terminal.
pub(crate) async fn run_criterion(
    cwd: &Path,
    cmd: &str,
    supervision: Supervision<'_>,
) -> Result<Precheck, ScopeExpansionError> {
    let command = GovernedCommand::shell(cwd, cmd)
        .stdout(Capture::Discard)
        .stderr(Capture::Discard);
    let outcome = spawn_governed(command, supervision)
        .await
        .map_err(|source| ScopeExpansionError::Process {
            cmd: cmd.to_string(),
            source,
        })?;
    Ok(match outcome {
        Outcome::Exited { status, .. } => match status.code().unwrap_or(-1) {
            0 => Precheck::Passes,
            exit => Precheck::Red { exit },
        },
        Outcome::TimedOut { .. } | Outcome::Cancelled { .. } => Precheck::CouldNotRun,
    })
}

/// Whether `printed` locates a write of one of `refused`, read the way the
/// attempt's close reads it: relative to `worktree`.
pub(crate) fn locates_a_refusal(printed: &str, worktree: &Path, refused: &[ToolTarget]) -> bool {
    let text = relative_text(printed.to_string(), worktree);
    refused
        .iter()
        .filter_map(|target| relative(worktree, target.display()?))
        .any(|path| locates(&text, &path))
}

/// `target` relative to the checkout, when it lies in it.
pub(crate) fn relative(worktree: &Path, target: &str) -> Option<String> {
    let path = Path::new(target);
    if path.is_relative() {
        return Some(target.to_string());
    }
    roots(worktree)
        .iter()
        .find_map(|root| path.strip_prefix(root).ok())
        .map(|inside| inside.to_string_lossy().into_owned())
}

/// The checkout's path as a session or a command may spell it: as the
/// run named it, and as the filesystem resolves it.
fn roots(worktree: &Path) -> Vec<PathBuf> {
    let mut roots = vec![worktree.to_path_buf()];
    roots.extend(worktree.canonicalize().ok().filter(|real| real != worktree));
    roots
}

/// `text` with the checkout's own path taken out, so a location reads
/// relative to it whichever way a command printed it.
pub(crate) fn relative_text(text: String, worktree: &Path) -> String {
    roots(worktree).iter().fold(text, |text, root| {
        text.replace(&format!("{}/", root.display()), "")
    })
}

/// Whether `text` locates `path`: names it whole — not as the tail of
/// another path, not by its stem — with a line number right after it, as
/// `path:12`.
pub(crate) fn locates(text: &str, path: &str) -> bool {
    text.match_indices(path).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = &text[at + path.len()..];
        let whole = before.is_none_or(|c| !(c.is_alphanumeric() || "_./-".contains(c)));
        let line = after
            .strip_prefix(':')
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
        whole && line
    })
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;
    use yunta_core::events::Decider;
    use yunta_core::expansion::{Denial, Proposal};
    use yunta_core::protected::Protected;
    use yunta_core::ScopeExpansionMode;

    use super::*;

    const CALLER: &str = "crates/testkit/src/bench/driving.rs";

    /// A checkout of one commit, with `files` written untracked on top of it.
    fn checkout(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        yunta_testkit::init_repo(dir.path());
        for file in files {
            std::fs::write(dir.path().join(file), "x").unwrap();
        }
        dir
    }

    /// The bounds of a `rules` node with no cap and no batch beside it.
    fn rules(protected: &Protected) -> Bounds<'_> {
        Bounds {
            mode: ScopeExpansionMode::Rules,
            max_files: 5,
            max_per_run: None,
            protected,
            beside: &[],
        }
    }

    /// A request for `paths` under `rules` whose proposed criterion is red.
    fn red_request(paths: &[ScopeGlob]) -> Ask<'_> {
        Ask::Requested {
            paths,
            reason: "small adjacent fix",
            proposal: Some(Proposal {
                cmd: "test -f nonexistent-marker",
                precheck: Precheck::Red { exit: 1 },
            }),
        }
    }

    #[tokio::test]
    async fn rules_mode_grants_a_small_in_bounds_request_with_a_red_criterion() {
        let dir = checkout(&["src.rs"]);
        let cancel = CancellationToken::new();
        let supervision = Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock);
        let protected = Protected::default();
        let paths = [ScopeGlob::from("src.rs")];
        let (decision, count) = expand(
            red_request(&paths),
            &rules(&protected),
            &GrantWindow::seeded(0),
            dir.path(),
            supervision,
        )
        .await
        .unwrap();
        assert!(
            matches!(
                decision,
                Decision::Grant {
                    by: Decider::Rule,
                    ..
                }
            ),
            "{decision:?}"
        );
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn rules_mode_denies_a_request_touching_too_many_files() {
        let names: Vec<String> = (0..10).map(|n| format!("f{n}.rs")).collect();
        let dir = checkout(&names.iter().map(String::as_str).collect::<Vec<_>>());
        let cancel = CancellationToken::new();
        let supervision = Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock);
        let protected = Protected::default();
        let paths = [ScopeGlob::from("f*.rs")];
        let (decision, count) = expand(
            red_request(&paths),
            &rules(&protected),
            &GrantWindow::seeded(0),
            dir.path(),
            supervision,
        )
        .await
        .unwrap();
        assert_eq!(
            decision,
            Decision::Deny(Denial::TooManyFiles {
                touched: 10,
                bound: 5
            })
        );
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn a_criterion_reads_its_exit_as_its_precheck() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let supervision = Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock);
        let passes = run_criterion(dir.path(), "true", supervision).await;
        assert_eq!(passes.unwrap(), Precheck::Passes);
        let red = run_criterion(dir.path(), "exit 3", supervision).await;
        assert_eq!(red.unwrap(), Precheck::Red { exit: 3 });
    }

    #[test]
    fn a_location_names_the_whole_path_and_a_line() {
        assert!(locates(
            "error[E0061]\n  --> crates/testkit/src/bench/driving.rs:378:9",
            CALLER
        ));
        assert!(locates(
            "at (crates/testkit/src/bench/driving.rs:12)",
            CALLER
        ));
    }

    #[test]
    fn stem_only_mention_is_not_evidence() {
        assert!(!locates("driving.rs:378", CALLER));
        assert!(!locates(
            "see crates/testkit/src/bench/driving.rs for details",
            CALLER
        ));
        assert!(!locates(
            "other/crates/testkit/src/bench/driving.rs:3",
            CALLER
        ));
        assert!(!locates("driving:3 failed", CALLER));
    }
}
