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
use yunta_core::{AdapterId, IdSource, Manifest, RunId, SystemClock};
use yunta_engine::{RunReport, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::AsyncStorage;

use crate::project::Project;

/// The CLI-side environment a promotion chain runs in — everything
/// [`drive_promotions`] needs that stays fixed across every successor it
/// creates, as opposed to the run-state it threads through the loop
/// (`run_id`/`manifest`/`worktree`/`report`, which change every
/// iteration and so stay their own arguments).
pub(crate) struct PromotionEnv<'a> {
    pub cwd: &'a Path,
    pub project: &'a Project,
    pub(crate) storage: &'a AsyncStorage,
    pub ids: &'a dyn IdSource,
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
        // what's the CLI's — the console surface, the system clock and
        // the id source.
        let successor = yunta_engine::create_promotion_successor(
            yunta_engine::Predecessor {
                id: &run_id,
                manifest: &manifest,
                worktree: &worktree,
                run_dir: &env.project.runs_root.join(run_id.as_str()),
            },
            env.cwd,
            &suggested_mode,
            yunta_engine::RunRoots {
                runs: &env.project.runs_root,
                worktrees: &env.project.worktrees_root,
            },
            env.storage,
            &SystemClock,
            env.ids,
        )
        .await
        .map_err(|e| e.to_string())?;
        println!(
            "run {run_id}: promoted to `{suggested_mode}` — starting {}",
            successor.run_id
        );

        let ambient = crate::project::process_env();
        let successor_report = yunta_engine::execute_run(yunta_engine::RunEnv {
            run_id: &successor.run_id,
            manifest: &successor.manifest,
            run_dir: &successor.run_dir,
            worktree: &successor.worktree,
            adapters: env.adapters,
            storage: env.storage,
            clock: std::sync::Arc::new(SystemClock),
            ids: env.ids,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &crate::human_interaction::ConsoleInteraction,
            forge: env.forge,
            cancel: env.cancel,
            adapter_override: None,
            ambient: Some(&ambient),
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

    use yunta_core::events::{EventPayload, GateWaitingPayload, HumanChoice};
    use yunta_core::{ConfigLayer, RunId, SystemClock, Workflow};
    use yunta_engine::{
        build_manifest, create_run, execute_run, HumanInteraction, RunEnv, DEFAULT_MAX_RETRIES,
    };
    use yunta_storage::Storage;

    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        assert!(
            yunta_engine::git::success_blocking(dir, args).unwrap(),
            "git {args:?} failed"
        );
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
        async fn resolve(&self, _e: &GateWaitingPayload) -> Option<HumanChoice> {
            Some(HumanChoice {
                option: "promote".into(),
                by: "eulke".into(),
                free_text: None,
            })
        }
    }

    /// `plan` produces a real artifact before `lint` sends the run to its
    /// escalation, so the successor has something to inherit.
    const WORKFLOW: &str = r#"
name: promotable
modes:
  quick:  { include: [plan, lint, fix-lint] }
  full:   { include: [ship] }
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks: []\n' > {{run.dir}}/artifacts/plan.yaml"
    artifacts: { produces: [plan.yaml] }
  - id: lint
    kind: bash
    depends_on: [plan]
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
                artifacts: &[],
            },
            &storage.async_handle(),
            &SystemClock,
        )
        .await
        .unwrap();

        let report = execute_run(RunEnv {
            run_id: &run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &worktree,
            adapters: &adapters,
            storage: &storage.async_handle(),
            clock: std::sync::Arc::new(SystemClock),
            ids: &yunta_core::SystemIdSource,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &AlwaysPromote,
            forge: None,
            cancel: None,
            adapter_override: None,
            ambient: None,
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
                storage: &storage.async_handle(),
                ids: &yunta_core::SystemIdSource,
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

        assert_ne!(final_id, run_id, "the successor is a run of its own");
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

        // The artifact the parent's log holds landed in the successor's
        // own dir, with no producer of the successor's behind it.
        let successor_run_dir = project.runs_root.join(final_id.as_str());
        assert_eq!(
            std::fs::read_to_string(successor_run_dir.join("artifacts").join("plan.yaml")).unwrap(),
            "tasks: []\n"
        );
    }
}
