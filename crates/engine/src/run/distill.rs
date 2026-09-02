//! `on_finish.distill`: a **deterministic
//! transform**, never an agent session — an LLM-written distillate at
//! close would be unverifiable content entering the knowledge layer
//! past every criterion and scope, the exact back door "la palabra del
//! agente no es evidencia" exists to close. A team that wants a
//! narrated summary produces it with a `prompt` node (its own runner,
//! budget and verification) and names *that artifact* here —
//! composition, not a new mechanism.
//!
//! Each declared artifact copies to
//! `<worktree>/.yunta/knowledge/distilled/<workflow>/<run_id>/` — one
//! subdirectory per run, **never a shared mutable index** (two
//! concurrent PRs distilling into one index is a guaranteed merge
//! conflict; the vice is avoided by construction) — with a
//! `provenance.yaml` derived purely from (manifest, log, clock): the
//! evidence a later reader checks the distillate against, not anyone's
//! word. Runs only at real closes (`Finish` and promotion — a short
//! attempt's knowledge is knowledge), before `run_finished` (nothing is
//! emitted after the close event), never at a pause.
//!
//! How it reaches the repo: under `isolation: worktree` the engine
//! commits the distilled files to the run's own branch
//! (`docs(knowledge): distill from <run_id>`) — the knowledge travels
//! in the same PR as the work and passes the same human review — and
//! pushes only if the branch already has an upstream. Under `none` the
//! files stay **uncommitted** in the user's checkout: the engine never
//! commits the user's branch; the visibly dirty tree that blocks the
//! next `none` run until a human commits or discards is deliberate
//! friction, not a bug.

use std::path::Path;

use serde::Serialize;
use yunta_core::events::{
    EventPayload, Finding, FindingPostedPayload, FindingSeverity, StoredEvent,
};
use yunta_core::{sha256_hex, FindingId, Isolation, ModeName, OnFinishStep};

use super::{RunCtx, RunError};

/// `provenance.yaml`'s whole document — serialized from structs so the
/// field order is fixed and the same inputs always give the same bytes.
#[derive(Serialize)]
struct Provenance {
    source_run: String,
    workflow: String,
    workflow_hash: String,
    mode: String,
    distilled_at: String,
    artifacts: Vec<ProvenanceArtifact>,
    verification: ProvenanceVerification,
}

#[derive(Serialize)]
struct ProvenanceArtifact {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    missing: bool,
}

#[derive(Serialize)]
struct ProvenanceVerification {
    criteria: CriteriaCounts,
    findings: FindingCounts,
}

#[derive(Serialize)]
struct CriteriaCounts {
    executed: usize,
    green: usize,
    reused: usize,
}

/// All four severities, not just the register's illustrative two — a
/// `major` finding vanishing from the evidence would be the exact
/// under-reporting this file exists to prevent.
#[derive(Serialize)]
struct FindingCounts {
    blocking: usize,
    major: usize,
    minor: usize,
    note: usize,
}

/// Log-derived verification evidence — pure over the event slice.
fn verification(events: &[StoredEvent]) -> ProvenanceVerification {
    let mut criteria = CriteriaCounts {
        executed: 0,
        green: 0,
        reused: 0,
    };
    let mut findings = FindingCounts {
        blocking: 0,
        major: 0,
        minor: 0,
        note: 0,
    };
    for event in events {
        match event.payload() {
            Some(EventPayload::CriteriaChecked(p)) => {
                for result in &p.results {
                    criteria.executed += 1;
                    if result.exit_code == 0 {
                        criteria.green += 1;
                    }
                    if result.reused {
                        criteria.reused += 1;
                    }
                }
            }
            Some(EventPayload::FindingPosted(p)) => match p.finding.severity {
                FindingSeverity::Blocking => findings.blocking += 1,
                FindingSeverity::Major => findings.major += 1,
                FindingSeverity::Minor => findings.minor += 1,
                FindingSeverity::Note => findings.note += 1,
            },
            _ => {}
        }
    }
    ProvenanceVerification { criteria, findings }
}

/// Runs the whole distill step for one closing run. Every failure past
/// the declared-path findings degrades loudly (a distill problem must
/// never un-close a run the log is about to close) — except IO on the
/// destination, which is a real error before `run_finished` exists.
pub(super) async fn run_distill(ctx: &RunCtx<'_>, mode: &ModeName) -> Result<(), RunError> {
    let declared: Vec<&String> = ctx
        .manifest
        .workflow
        .on_finish
        .iter()
        .filter_map(|step| match step {
            OnFinishStep::Distill { distill } => Some(distill.iter()),
            _ => None,
        })
        .flatten()
        .collect();
    if declared.is_empty() {
        return Ok(());
    }

    let dest_dir = ctx
        .worktree
        .join(".yunta/knowledge/distilled")
        .join(&ctx.manifest.workflow.name)
        .join(ctx.run_id.as_str());
    std::fs::create_dir_all(&dest_dir).map_err(|source| RunError::Io {
        context: format!("create `{}`", dest_dir.display()),
        source,
    })?;

    let mut artifacts = Vec::new();
    for name in &declared {
        let source = ctx.run_dir.join("artifacts").join(name.as_str());
        match std::fs::read(&source) {
            Ok(bytes) => {
                let dest = dest_dir.join(name.as_str());
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|source| RunError::Io {
                        context: format!("create `{}`", parent.display()),
                        source,
                    })?;
                }
                std::fs::write(&dest, &bytes).map_err(|source| RunError::Io {
                    context: format!("write `{}`", dest.display()),
                    source,
                })?;
                artifacts.push(ProvenanceArtifact {
                    name: name.to_string(),
                    content_hash: Some(format!("sha256:{}", sha256_hex(&bytes))),
                    missing: false,
                });
            }
            Err(_) => {
                // Declared durable, never produced (a mode excluded its
                // node, a reroute never reached it): registered, never
                // lost — reported as a finding — and the rest distills
                // anyway.
                ctx.emit(
                    None,
                    EventPayload::FindingPosted(FindingPostedPayload {
                        finding: Finding {
                            id: FindingId::try_from(format!("distill-missing-{name}"))?,
                            severity: FindingSeverity::Minor,
                            title: format!(
                                "distill: declared artifact `{name}` was never produced"
                            ),
                            location: format!("run.dir/artifacts/{name}"),
                            detail: "the workflow's `on_finish.distill` names this artifact \
                                     but no node produced it in this run"
                                .to_string(),
                            proposed_criterion: None,
                        },
                    }),
                )
                .await?;
                artifacts.push(ProvenanceArtifact {
                    name: name.to_string(),
                    content_hash: None,
                    missing: true,
                });
            }
        }
    }

    let provenance = Provenance {
        source_run: ctx.run_id.to_string(),
        workflow: ctx.manifest.workflow.name.clone(),
        workflow_hash: format!("sha256:{}", ctx.manifest.workflow_hash),
        mode: mode.to_string(),
        distilled_at: ctx.clock.now().to_rfc3339(),
        artifacts,
        verification: verification(&ctx.load_events().await?),
    };
    let yaml = yunta_core::yaml::to_string(&provenance).map_err(|e| RunError::Broken {
        diagnostic: format!("failed to serialize distill provenance: {e}"),
    })?;
    let provenance_path = dest_dir.join("provenance.yaml");
    std::fs::write(&provenance_path, yaml).map_err(|source| RunError::Io {
        context: format!("write `{}`", provenance_path.display()),
        source,
    })?;

    if ctx.manifest.isolation == Isolation::Worktree {
        commit_and_maybe_push(ctx.worktree, ctx.run_id.as_str()).await;
    }
    Ok(())
}

/// The knowledge travels on the run's own branch. Failures here warn —
/// the files are on disk either way, and `cleanup`'s `git branch -d`
/// (never `-D`) already guarantees an unpushed distill commit can't be
/// destroyed by the close.
async fn commit_and_maybe_push(worktree: &Path, run_id: &str) {
    let git = |args: Vec<String>| {
        let worktree = worktree.to_path_buf();
        async move {
            tokio::process::Command::new("git")
                .args(&args)
                .current_dir(&worktree)
                .output()
                .await
        }
    };
    let add = git(vec![
        "add".to_string(),
        ".yunta/knowledge/distilled".to_string(),
    ])
    .await;
    if !matches!(&add, Ok(output) if output.status.success()) {
        tracing::warn!("distill: `git add` failed — the files stay uncommitted on disk");
        return;
    }
    let commit = git(vec![
        "commit".to_string(),
        "-m".to_string(),
        format!("docs(knowledge): distill from {run_id}"),
    ])
    .await;
    if !matches!(&commit, Ok(output) if output.status.success()) {
        tracing::warn!("distill: `git commit` failed — the files stay uncommitted on disk");
        return;
    }
    // Push only where an upstream already exists (a `pr` node's own
    // `push -u`); otherwise the commit rides the local branch, which
    // cleanup's `-d` refuses to delete unmerged.
    let upstream = git(vec![
        "rev-parse".to_string(),
        "--abbrev-ref".to_string(),
        "--symbolic-full-name".to_string(),
        "@{u}".to_string(),
    ])
    .await;
    if matches!(&upstream, Ok(output) if output.status.success()) {
        let push = git(vec!["push".to_string()]).await;
        if !matches!(&push, Ok(output) if output.status.success()) {
            tracing::warn!("distill: `git push` failed — the commit stays on the local branch");
        }
    }
}
