//! Drives a `run`/`resume` call through a promotion chain (§10.2, D22,
//! T9.2): `execute_run` itself only ever closes *one* run and hands back
//! `RunTerminal::Promoted { suggested_mode }` — creating and starting
//! the successor needs the original repo checkout (`cwd`) to prepare a
//! worktree, which `execute_run` is never given (§7.3: it only ever
//! receives an *already-prepared* one). `run`/`resume` call
//! [`drive_promotions`] right after their own `execute_run`, and use
//! its returned `(run_id, manifest, worktree, report)` — the *last*
//! run in the chain — for everything after (release/report).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_adapters::{Adapter, Forge};
use yunta_core::{Isolation, Manifest, RunId, SystemClock};
use yunta_engine::{RunReport, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::Storage;

use crate::project::Project;

fn current_head(repo: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .map_err(|e| {
            format!(
                "failed to run `git rev-parse HEAD` in `{}`: {e}",
                repo.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "`git rev-parse HEAD` in `{}` failed: {}",
            repo.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Copies every file under `from`'s `artifacts/` into `to`'s own —
/// §10.2's "el contexto inicial incluye automáticamente artifacts,
/// ledger y findings del antecesor", done at the filesystem level: a
/// `kind: task-ledger` ledger and any `kind: findings` artifact are
/// both just files under `artifacts/`, so copying the directory covers
/// both without a dedicated cross-run `ContextSource`. **Deliberately
/// narrower than §12's own general "runs vinculados" mechanism** (which
/// would let a node explicitly reference a *specific* linked run's
/// artifact by id) — that shared infrastructure is T9.3's own
/// (`kind: workflow` composition needs the identical capability, and
/// building it once, generally, when T9.3 lands is better than a
/// throwaway promotion-only version now that would just get replaced.
/// See `docs/m0-status.md`'s T9.2 entry.
fn copy_inherited_artifacts(from_run_dir: &Path, to_run_dir: &Path) -> std::io::Result<()> {
    let from = from_run_dir.join("artifacts");
    let to = to_run_dir.join("artifacts");
    if !from.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&from)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Runs the whole promotion chain to its end: while the latest
/// `execute_run` call returned `Promoted`, creates and starts the
/// successor, then checks *its* terminal in turn. Bounded automatically
/// — `modes:` is a finite, strictly-forward-only ladder (§10.1), so this
/// can run at most `len(modes) - 1` times before landing on a mode with
/// nowhere further to promote to.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn drive_promotions(
    cwd: &Path,
    project: &Project,
    storage: &Storage,
    adapters: &HashMap<String, Arc<dyn Adapter>>,
    forge: Option<&dyn Forge>,
    mut run_id: RunId,
    mut manifest: Manifest,
    mut worktree: PathBuf,
    mut report: RunReport,
) -> Result<(RunId, Manifest, PathBuf, RunReport), String> {
    while let RunTerminal::Promoted { suggested_mode } = &report.terminal {
        let suggested_mode = suggested_mode.clone();
        let successor_id = RunId::from(format!("{run_id}-promoted"));
        println!("run {run_id}: promoted to `{suggested_mode}` — starting {successor_id}");

        let mut successor_manifest = manifest.clone();
        successor_manifest.base_commit = current_head(&worktree)?;

        let successor_worktree = match manifest.isolation {
            // §7.3: a fresh worktree per promotion, from wherever the
            // parent's own work left the tree — never a new lock
            // contest on `cwd` under `none` (the parent's lock is still
            // held; it's only released once the *whole* chain finishes).
            Isolation::Worktree => project.worktrees_root.join(successor_id.as_str()),
            Isolation::None => cwd.to_path_buf(),
        };
        if manifest.isolation == Isolation::Worktree {
            yunta_engine::prepare_worktree(
                cwd,
                &successor_worktree,
                &successor_manifest.base_commit,
                &format!("yunta/{successor_id}"),
                manifest.isolation,
            )
            .await
            .map_err(|e| e.to_string())?;
        }

        let successor_run_dir = yunta_engine::create_run(
            &successor_id,
            &successor_manifest,
            &project.runs_root,
            storage,
            &SystemClock,
            &suggested_mode,
            Some(&run_id),
        )
        .map_err(|e| e.to_string())?;

        let parent_run_dir = project.runs_root.join(run_id.as_str());
        copy_inherited_artifacts(&parent_run_dir, &successor_run_dir)
            .map_err(|e| format!("failed to inherit artifacts from `{run_id}`: {e}"))?;

        let successor_report = yunta_engine::execute_run(
            &successor_id,
            &successor_manifest,
            &successor_run_dir,
            &successor_worktree,
            adapters,
            storage,
            &SystemClock,
            DEFAULT_MAX_RETRIES,
            &crate::human_interaction::ConsoleInteraction,
            forge,
        )
        .await
        .map_err(|e| e.to_string())?;

        run_id = successor_id;
        manifest = successor_manifest;
        worktree = successor_worktree;
        report = successor_report;
    }
    Ok((run_id, manifest, worktree, report))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use yunta_core::events::{EventPayload, GateResolvedPayload, GateWaitingPayload};
    use yunta_core::{ConfigLayer, RunId, SystemClock, Workflow};
    use yunta_engine::{
        build_manifest, create_run, execute_run, HumanInteraction, DEFAULT_MAX_RETRIES,
    };
    use yunta_storage::Storage;

    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "test@example.com"]);
        git(dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join(".gitkeep"), "").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "initial"]);
    }

    struct AlwaysPromote;

    #[async_trait::async_trait]
    impl HumanInteraction for AlwaysPromote {
        async fn resolve(&self, _e: &GateWaitingPayload) -> Option<GateResolvedPayload> {
            Some(GateResolvedPayload {
                chosen_option: Some("promote".to_string()),
                resolved_by: Some("eulke".to_string()),
                free_text: None,
                approved_sha: None,
            })
        }
    }

    const WORKFLOW: &str = r#"
name: promotable
modes:
  quick:  { include: [lint, fix-lint] }
  full:   { include: [ship] }
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
  - id: ship
    kind: bash
    run: "true"
"#;

    #[tokio::test]
    async fn drive_promotions_creates_and_runs_a_successor_with_the_chain_audited() {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("repo");
        std::fs::create_dir_all(&cwd).unwrap();
        init_repo(&cwd);
        // A real artifact on the parent — proves inheritance, not just
        // an empty directory copy succeeding trivially.
        std::fs::create_dir_all(cwd.join(".yunta")).unwrap();

        let project = Project {
            config: ConfigLayer::default(),
            runs_root: root.path().join("runs"),
            worktrees_root: root.path().join("worktrees"),
            storage_path: root.path().join("yunta.db"),
        };
        let storage = Storage::open(&project.storage_path).unwrap();
        let adapters: HashMap<String, std::sync::Arc<dyn Adapter>> = HashMap::new();

        let workflow: Workflow = serde_yaml::from_str(WORKFLOW).unwrap();
        let manifest =
            build_manifest(&workflow, &project.config, &cwd, &cwd, &HashMap::new()).unwrap();

        let run_id = RunId::from("run-parent");
        let worktree = project.worktrees_root.join(run_id.as_str());
        yunta_engine::prepare_worktree(
            &cwd,
            &worktree,
            &manifest.base_commit,
            &format!("yunta/{run_id}"),
            manifest.isolation,
        )
        .await
        .unwrap();
        let run_dir = create_run(
            &run_id,
            &manifest,
            &project.runs_root,
            &storage,
            &SystemClock,
            "quick",
            None,
        )
        .unwrap();

        // A real artifact file on the parent's own run.dir — what
        // `copy_inherited_artifacts` is supposed to carry forward.
        std::fs::write(run_dir.join("artifacts").join("plan.yaml"), "tasks: []\n").unwrap();

        let report = execute_run(
            &run_id,
            &manifest,
            &run_dir,
            &worktree,
            &adapters,
            &storage,
            &SystemClock,
            DEFAULT_MAX_RETRIES,
            &AlwaysPromote,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(
            report.terminal,
            yunta_engine::RunTerminal::Promoted { .. }
        ));

        let (final_id, _final_manifest, _final_worktree, final_report) = drive_promotions(
            &cwd,
            &project,
            &storage,
            &adapters,
            None,
            run_id.clone(),
            manifest,
            worktree,
            report,
        )
        .await
        .unwrap();

        assert_eq!(final_id, RunId::from("run-parent-promoted"));
        assert_eq!(final_report.terminal, yunta_engine::RunTerminal::Finished);

        // Chain audited on the successor's own log: run_created carries
        // promoted_from back to the exact parent run_id.
        let successor_events = storage.events_for_run(&final_id).unwrap();
        let created = successor_events.iter().find_map(|e| match &e.payload {
            EventPayload::RunCreated(p) => Some(p),
            _ => None,
        });
        assert_eq!(created.unwrap().promoted_from, Some(run_id.clone()));
        assert_eq!(created.unwrap().mode, "full");

        // And on the parent's own log — already asserted at the engine
        // level (promotion.rs), reconfirmed here as the visible half of
        // "cadena auditada en ambos logs".
        let parent_events = storage.events_for_run(&run_id).unwrap();
        assert!(parent_events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::PromotionSignaled(_))));

        // Inherited artifact actually landed in the successor's own dir.
        let successor_run_dir = project.runs_root.join(final_id.as_str());
        assert!(successor_run_dir
            .join("artifacts")
            .join("plan.yaml")
            .exists());
    }
}
