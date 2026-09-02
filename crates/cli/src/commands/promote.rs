//! Drives a `run`/`resume` call through a promotion chain: `execute_run`
//! itself only ever closes *one* run and hands back
//! `RunTerminal::Promoted { suggested_mode }` — creating and starting
//! the successor needs the original repo checkout (`cwd`) to prepare a
//! worktree, which `execute_run` is never given — it only ever receives
//! an *already-prepared* one. `run`/`resume` call
//! [`drive_promotions`] right after their own `execute_run`, and use
//! its returned `(run_id, manifest, worktree, report)` — the *last*
//! run in the chain — for everything after (release/report).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_adapters::{Adapter, Forge};
use yunta_core::{AdapterId, Manifest, RunId, SystemClock};
use yunta_engine::{RunReport, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::Storage;

use crate::project::Project;

/// The CLI-side environment a promotion chain runs in — everything
/// [`drive_promotions`] needs that stays fixed across every successor it
/// creates, as opposed to the run-state it threads through the loop
/// (`run_id`/`manifest`/`worktree`/`report`, which change every
/// iteration and so stay their own arguments).
pub(crate) struct PromotionEnv<'a> {
    pub cwd: &'a Path,
    pub project: &'a Project,
    pub storage: &'a Storage,
    pub adapters: &'a HashMap<AdapterId, Arc<dyn Adapter>>,
    pub forge: Option<&'a dyn Forge>,
    pub cancel: Option<&'a tokio_util::sync::CancellationToken>,
}

/// Runs the whole promotion chain to its end: while the latest
/// `execute_run` call returned `Promoted`, creates and starts the
/// successor, then checks *its* terminal in turn. Bounded automatically
/// — `modes:` is a finite, strictly-forward-only ladder, so this can
/// run at most `len(modes) - 1` times before landing on a mode with
/// nowhere further to promote to.
pub(crate) async fn drive_promotions(
    env: &PromotionEnv<'_>,
    mut run_id: RunId,
    mut manifest: Manifest,
    mut worktree: PathBuf,
    mut report: RunReport,
) -> Result<(RunId, Manifest, PathBuf, RunReport), String> {
    while let RunTerminal::Promoted { suggested_mode } = &report.terminal {
        let suggested_mode = suggested_mode.clone();
        // The creation mechanics live in the engine (shared with
        // `kind: workflow` children that promote); this loop keeps only
        // what's the CLI's — the console surface and the system clock.
        let successor = yunta_engine::create_promotion_successor(
            yunta_engine::Predecessor {
                id: &run_id,
                manifest: &manifest,
                worktree: &worktree,
                run_dir: &env.project.runs_root.join(run_id.as_str()),
            },
            env.cwd,
            &suggested_mode,
            &env.project.runs_root,
            &env.project.worktrees_root,
            env.storage,
            &SystemClock,
        )
        .await
        .map_err(|e| e.to_string())?;
        println!(
            "run {run_id}: promoted to `{suggested_mode}` — starting {}",
            successor.run_id
        );

        let successor_report = yunta_engine::execute_run(yunta_engine::RunEnv {
            run_id: &successor.run_id,
            manifest: &successor.manifest,
            run_dir: &successor.run_dir,
            worktree: &successor.worktree,
            adapters: env.adapters,
            storage: env.storage,
            clock: &SystemClock,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &crate::human_interaction::ConsoleInteraction,
            forge: env.forge,
            cancel: env.cancel,
            adapter_override: None,
        })
        .await
        .map_err(|e| e.to_string())?;

        run_id = successor.run_id;
        manifest = successor.manifest;
        worktree = successor.worktree;
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
        build_manifest, create_run, execute_run, HumanInteraction, RunEnv, DEFAULT_MAX_RETRIES,
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
        let adapters: HashMap<AdapterId, std::sync::Arc<dyn Adapter>> = HashMap::new();

        let workflow: Workflow = yunta_core::yaml::parse(WORKFLOW).unwrap();
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
            yunta_engine::CreateRunParams {
                run_id: &run_id,
                manifest: &manifest,
                runs_root: &project.runs_root,
                mode: &"quick".into(),
                promoted_from: None,
            },
            &storage,
            &SystemClock,
        )
        .unwrap();

        // A real artifact file on the parent's own run.dir — what
        // `copy_inherited_artifacts` is supposed to carry forward.
        std::fs::write(run_dir.join("artifacts").join("plan.yaml"), "tasks: []\n").unwrap();

        let report = execute_run(RunEnv {
            run_id: &run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &worktree,
            adapters: &adapters,
            storage: &storage,
            clock: &SystemClock,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &AlwaysPromote,
            forge: None,
            cancel: None,
            adapter_override: None,
        })
        .await
        .unwrap();
        assert!(matches!(
            report.terminal,
            yunta_engine::RunTerminal::Promoted { .. }
        ));

        let (final_id, _final_manifest, _final_worktree, final_report) = drive_promotions(
            &PromotionEnv {
                cwd: &cwd,
                project: &project,
                storage: &storage,
                adapters: &adapters,
                forge: None,
                cancel: None,
            },
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
        let created = successor_events.iter().find_map(|e| match e.payload() {
            Some(EventPayload::RunCreated(p)) => Some(p),
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
            .any(|e| matches!(e.payload(), Some(EventPayload::PromotionSignaled(_)))));

        // Inherited artifact actually landed in the successor's own dir.
        let successor_run_dir = project.runs_root.join(final_id.as_str());
        assert!(successor_run_dir
            .join("artifacts")
            .join("plan.yaml")
            .exists());
    }
}
