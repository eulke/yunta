//! Driving a run on a [`Bench`]: creating it, and every wake that
//! executes it.
//!
//! A wake is one `execute_run` call, exactly as a separate `yunta run`
//! or `yunta resume` process makes it: nothing carries across a wake
//! but the log.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::{MockAdapter, MockFixture, RunPaths};
use yunta_core::port::Adapter;
use yunta_core::{AdapterId, ConfigLayer, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, BirthArtifact, CreateRunParams, HumanInteraction,
    NoInteraction, RunEnv, RunReport, DEFAULT_MAX_RETRIES,
};

use super::{Bench, Driven, MOCK_CONFIG};

impl Bench {
    /// Runs `(workflow, fixture)` against the default [`MOCK_CONFIG`] with
    /// no human present (a gate degrades to pause).
    pub async fn run(&self, workflow_yaml: &str, fixture_yaml: &str) -> RunReport {
        self.run_full(workflow_yaml, fixture_yaml, MOCK_CONFIG, &NoInteraction)
            .await
    }

    /// Runs with a caller-chosen config layer — for a test that needs
    /// `baseline:`/`coverage:`/`limits:` alongside the usual `runners:`.
    pub async fn run_with_config(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
    ) -> RunReport {
        self.run_full(workflow_yaml, fixture_yaml, config_yaml, &NoInteraction)
            .await
    }

    /// Runs with the secrets a test hands it, instead of reaching the
    /// process environment: the values are the test's to choose, so what
    /// happens to them is the test's to assert.
    pub async fn run_with_secrets(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
        secrets: &[(&str, &str)],
    ) -> RunReport {
        let known: std::collections::HashMap<String, String> = secrets
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        self.run_with(
            workflow_yaml,
            fixture_yaml,
            config_yaml,
            &NoInteraction,
            Some(std::sync::Arc::new(KnownSecrets(known))),
        )
        .await
    }

    /// Runs with a caller-chosen interaction surface — for a test that
    /// scripts a gate's resolution instead of degrading to pause.
    pub async fn run_with_interaction(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        human_interaction: &dyn HumanInteraction,
    ) -> RunReport {
        self.run_full(workflow_yaml, fixture_yaml, MOCK_CONFIG, human_interaction)
            .await
    }

    /// The fully parameterized run every other `run*` helper delegates to.
    pub async fn run_full(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
        human_interaction: &dyn HumanInteraction,
    ) -> RunReport {
        self.run_with(
            workflow_yaml,
            fixture_yaml,
            config_yaml,
            human_interaction,
            None,
        )
        .await
    }

    /// The one body every `run…` helper ends in: the run is created,
    /// then woken once — the two steps a `yunta run` takes.
    async fn run_with(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
        human_interaction: &dyn HumanInteraction,
        secrets: Option<Arc<dyn yunta_core::SecretSource>>,
    ) -> RunReport {
        self.create(workflow_yaml, fixture_yaml, config_yaml).await;
        self.wake_with_all(human_interaction, secrets).await
    }

    /// Runs `(workflow, fixture)` and answers with the refusal instead
    /// of panicking on it — for a test about what a run's own creation
    /// or execution says no to.
    pub async fn try_run(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
    ) -> Result<RunReport, yunta_engine::RunError> {
        self.try_create(workflow_yaml, fixture_yaml, MOCK_CONFIG)
            .await?;
        self.wake_through(&NoInteraction, None).await
    }

    /// Creates the run and stops there, answering with its directory or
    /// with the refusal its birth earned — for a test about what a run
    /// is born holding, before anything executes.
    pub async fn birth(
        &self,
        workflow_yaml: &str,
    ) -> Result<std::path::PathBuf, yunta_engine::RunError> {
        self.try_create(workflow_yaml, "sessions: []\n", MOCK_CONFIG)
            .await
    }

    /// Executes the run this bench already created, keeping the refusal
    /// a wake earns.
    pub async fn try_wake(&self) -> Result<RunReport, yunta_engine::RunError> {
        self.wake_through(&NoInteraction, None).await
    }

    /// The same, with a human answering what the run asks.
    pub async fn try_wake_answering(
        &self,
        human_interaction: &dyn HumanInteraction,
    ) -> Result<RunReport, yunta_engine::RunError> {
        self.wake_through(human_interaction, None).await
    }

    /// Runs `(workflow, fixture)` with `sabotage` handed the run
    /// directory between the run's creation and its first wake — the
    /// window a machine loses a file in, and the only place a test can
    /// open it.
    pub async fn run_sabotaged(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        sabotage: impl FnOnce(&std::path::Path),
    ) -> RunReport {
        self.run_sabotaged_answering(workflow_yaml, fixture_yaml, &NoInteraction, sabotage)
            .await
    }

    /// The same, with a human answering what the run asks.
    pub async fn run_sabotaged_answering(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        human_interaction: &dyn HumanInteraction,
        sabotage: impl FnOnce(&std::path::Path),
    ) -> RunReport {
        let run_dir = self.create(workflow_yaml, fixture_yaml, MOCK_CONFIG).await;
        sabotage(&run_dir);
        self.wake_with_all(human_interaction, None).await
    }

    /// The same, keeping the refusal the sabotaged wake earns.
    pub async fn try_run_sabotaged(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        sabotage: impl FnOnce(&std::path::Path),
    ) -> Result<RunReport, yunta_engine::RunError> {
        let run_dir = self
            .try_create(workflow_yaml, fixture_yaml, MOCK_CONFIG)
            .await?;
        sabotage(&run_dir);
        self.wake_through(&NoInteraction, None).await
    }

    /// Executes the run this bench already created, again.
    ///
    /// A fresh call, exactly like the separate `yunta resume` process
    /// that makes one: nothing carries across a wake but the log.
    pub async fn wake(&self) -> RunReport {
        self.wake_with_all(&NoInteraction, None).await
    }

    /// The same wake, its sessions answering from a fresh fixture —
    /// what a separate resume process does, since it builds its own
    /// adapter from the fixture it was given.
    pub async fn wake_on_fixture(&self, fixture_yaml: &str) -> RunReport {
        self.wake_on_fixture_answering(fixture_yaml, &NoInteraction)
            .await
    }

    /// The same, with a human answering what the run asks.
    pub async fn wake_on_fixture_answering(
        &self,
        fixture_yaml: &str,
        human_interaction: &dyn HumanInteraction,
    ) -> RunReport {
        let run_dir = self.driven(|driven| driven.run_dir.clone());
        let mock = self.mock_from(fixture_yaml, &run_dir);
        {
            let mut driven = self.driven.lock().expect("the bench's own lock");
            let driven = driven.as_mut().expect("a run to wake");
            driven
                .adapters
                .insert("mock".into(), mock.clone() as Arc<dyn Adapter>);
            driven.mock = mock;
        }
        self.wake_with_all(human_interaction, None).await
    }

    /// The same wake, with a human answering — what resolves a gate
    /// instead of letting it degrade to a pause.
    pub async fn wake_answering(&self, human_interaction: &dyn HumanInteraction) -> RunReport {
        self.wake_with_all(human_interaction, None).await
    }

    /// Creates the run and records what waking it takes, answering with
    /// its run directory.
    pub async fn create(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
    ) -> std::path::PathBuf {
        self.try_create(workflow_yaml, fixture_yaml, config_yaml)
            .await
            .expect("create run")
    }

    /// The same, answering with the refusal a creation earns.
    pub async fn try_create(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
    ) -> Result<std::path::PathBuf, yunta_engine::RunError> {
        self.try_create_after(workflow_yaml, fixture_yaml, config_yaml, || {})
            .await
    }

    /// Creates the run with `between` run after the manifest is frozen
    /// and before the birth writes anything — where a test states what
    /// must be true of the world by the time the run is actually born,
    /// and not a moment earlier.
    pub async fn try_create_after(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
        config_yaml: &str,
        between: impl FnOnce(),
    ) -> Result<std::path::PathBuf, yunta_engine::RunError> {
        let workflow: Workflow = serde_norway::from_str(workflow_yaml).expect("parse workflow");
        let config: ConfigLayer = serde_norway::from_str(config_yaml).expect("parse config");
        let frozen = self.freeze(&workflow, &config).await;
        // A `type: document` input is born as an artifact, so the run a
        // bench creates holds what its `inputs:` named alongside
        // whatever the test hands it.
        let artifacts: Vec<BirthArtifact> =
            self.birth.iter().cloned().chain(frozen.documents).collect();
        let manifest = frozen.manifest;
        between();

        let run_dir = create_run(
            CreateRunParams {
                run_id: &self.run_id,
                manifest: &manifest,
                runs_root: &self.runs_root,
                mode: &self.mode,
                worktree: &self.worktree,
                promoted_from: None,
                artifacts: &artifacts,
                // A run a caller starts is the root of its lineage: it
                // measures on its first wake, if its config names a
                // suite.
                baseline: None,
            },
            &self.storage.async_handle(),
            self.supervision(),
        )
        .await?;

        let mock = self.mock_from(fixture_yaml, &run_dir);
        let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), mock.clone() as Arc<dyn Adapter>);
        *self.driven.lock().expect("the bench's own lock") = Some(Driven {
            manifest,
            run_dir: run_dir.clone(),
            adapters,
            mock,
        });
        Ok(run_dir)
    }

    /// The mock a fixture scripts, against the directories the run owns.
    fn mock_from(&self, fixture_yaml: &str, run_dir: &std::path::Path) -> Arc<MockAdapter> {
        Arc::new(MockAdapter::new(
            MockFixture::parse(
                fixture_yaml,
                &RunPaths {
                    run_dir,
                    worktree: &self.worktree,
                    staging: &yunta_engine::run_dir::staging_root(run_dir),
                },
            )
            .expect("parse mock fixture"),
        ))
    }

    /// The one `execute_run` this harness makes, its refusal kept.
    async fn wake_through(
        &self,
        human_interaction: &dyn HumanInteraction,
        secrets: Option<Arc<dyn yunta_core::SecretSource>>,
    ) -> Result<RunReport, yunta_engine::RunError> {
        let (manifest, run_dir, adapters) = self.driven(|driven| {
            (
                driven.manifest.clone(),
                driven.run_dir.clone(),
                driven.adapters.clone(),
            )
        });
        execute_run(RunEnv {
            run_id: &self.run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage.async_handle(),
            clock: self.clock.clone(),
            ids: self.ids.as_ref(),
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction,
            forge: self.forge.as_deref(),
            cancel: &self.cancel,
            adapter_override: None,
            ambient: self.ambient.as_ref(),
            secrets,
            // The bench runs no binary, so nothing can run a hook:
            // an adapter whose fence needs one fails the session, and
            // the mock judges in process.
            fence_hook: None,
            observer: self.observer.clone(),
        })
        .await
    }

    /// The same wake, panicking on a refusal no test is about.
    async fn wake_with_all(
        &self,
        human_interaction: &dyn HumanInteraction,
        secrets: Option<Arc<dyn yunta_core::SecretSource>>,
    ) -> RunReport {
        self.wake_through(human_interaction, secrets)
            .await
            .expect("execute run")
    }

    /// What `(workflow, config)` freezes to against this bench's world:
    /// the manifest the run carries, and the documents its `inputs:`
    /// named.
    pub(super) async fn freeze(
        &self,
        workflow: &Workflow,
        config: &ConfigLayer,
    ) -> yunta_engine::FrozenRun {
        build_manifest(
            workflow,
            config,
            self.workflow_dir.as_deref().unwrap_or(&self.worktree),
            &self.worktree,
            &self.inputs,
            self.supervision(),
        )
        .await
        .expect("build manifest")
    }
}

/// The secrets a test chose, instead of whatever the process happens to
/// carry: a test that reached the real environment would assert about a
/// machine rather than about the engine.
struct KnownSecrets(std::collections::HashMap<String, String>);

impl yunta_core::SecretSource for KnownSecrets {
    fn get(&self, name: &str) -> Option<yunta_core::Secret<String>> {
        self.0.get(name).cloned().map(yunta_core::Secret::from)
    }
}
