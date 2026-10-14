//! The registry a run's sessions run on, and what this binary refuses before
//! a run exists or a resume takes anything. Every invocation that drives a
//! run reaches its adapters through here: `run`, `resume`, a detached child,
//! a test case and a promotion successor each take the registry their
//! manifest names, so a run behaves the same whichever of them drives it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_adapters::{FixtureError, MockAdapter, MockFixture, RunPaths};
use yunta_core::{
    AdapterId, Clock, Manifest, ModeName, MountSpec, Node, NodeId, NodeKind, Redactor, RunAdapter,
    RunId, RunnerName, Workflow,
};
use yunta_engine::{CreateRunParams, FrozenRun, RunEnv, RunError, RunReport};
use yunta_storage::AsyncStorage;

use super::Adapters;
use crate::context::Context;
use crate::error::CliError;

/// What `servable` refuses. Each variant names the node, runner or adapter
/// that cannot be served, so the person reading it knows what to change.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ServableError {
    /// The run is pinned to an adapter that a runner the node uses does not
    /// reach: none of that runner's candidates is on it.
    #[error(
        "node `{node}` runs on runner `{runner}`, which has no candidate on `--adapter {pin}` — \
         its candidates are on: {}; add one on `{pin}` to `runners.{runner}`, or drop the flag",
        listed(candidates)
    )]
    NoCandidateOnPin {
        node: NodeId,
        runner: RunnerName,
        pin: AdapterId,
        candidates: Vec<AdapterId>,
    },
    /// The pinned adapter is not one `runners:` names that this binary builds.
    /// `runnable` is what the refusal offers instead: those adapters, sorted.
    #[error(
        "unknown adapter `{pin}` — this binary can run: {}",
        runnable_list(runnable)
    )]
    UnbuiltPin {
        pin: AdapterId,
        runnable: Vec<AdapterId>,
    },
    /// A mock run written before runs froze their adapter. Its log shows it
    /// was scripted, and nothing kept the fixture it was scripted from.
    #[error(
        "this run used the mock before runs froze their adapter, and its fixture was not kept; \
         it cannot resume"
    )]
    MockBeforeFrozenAdapter,
}

/// The adapters a runner's candidates are on, comma-separated, or `none` for a
/// runner with no candidate at all.
fn listed(adapters: &[AdapterId]) -> String {
    if adapters.is_empty() {
        return "none".to_string();
    }
    adapters
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// What an unbuilt pin is refused in favor of: the adapters `runners:` names
/// that this binary builds, or a note that `runners:` names none it builds.
fn runnable_list(runnable: &[AdapterId]) -> String {
    if runnable.is_empty() {
        return "(none configured — `runners:` names no adapter this build supports)".to_string();
    }
    listed(runnable)
}

impl From<ServableError> for CliError {
    fn from(error: ServableError) -> Self {
        CliError::msg(error.to_string())
    }
}

/// Refuses what this binary cannot serve, before a run exists or a token is
/// spent. Walks every node the run reaches (fan-out and `parallel` children
/// included, and each frozen `use:` child's). NotPinned stays a node-time
/// cause.
pub(crate) async fn servable(manifest: &Manifest) -> Result<(), CliError> {
    let pin = manifest.adapter.as_ref().and_then(RunAdapter::pinned);
    let children = frozen_children(manifest)?;
    let nodes = reached_nodes(manifest, &children);
    refuse_unlisted_pins(manifest, &nodes, pin)?;
    // A mock run's sessions are scripted: no real adapter is built, named or
    // probed for it.
    if !matches!(manifest.adapter, Some(RunAdapter::Mock { .. })) {
        let adapters = super::real_adapters(&manifest.config);
        if let Some(pin) = pin.filter(|pin| !adapters.contains_key(*pin)) {
            let mut runnable: Vec<AdapterId> = adapters.keys().cloned().collect();
            runnable.sort();
            return Err(ServableError::UnbuiltPin {
                pin: pin.clone(),
                runnable,
            }
            .into());
        }
        super::refuse_unrunnable(&manifest.workflow, &adapters)?;
        refuse_unchecked(manifest, &children, &nodes)?;
        super::probe_or_refuse(&adapters).await?;
    }
    Ok(())
}

/// Refuses the first node that takes a session on a runner with no candidate
/// on the pinned adapter, naming the node, its runner and the candidates.
fn refuse_unlisted_pins(
    manifest: &Manifest,
    nodes: &[&Node],
    pin: Option<&AdapterId>,
) -> Result<(), CliError> {
    let Some(pin) = pin else {
        return Ok(());
    };
    for node in nodes {
        // Only a node that takes a session resolves a runner; no other node
        // reaches one, whatever `defaults.runner` names.
        if !node.opens_session() {
            continue;
        }
        let Some(role) = manifest.config.runner_role(node) else {
            continue;
        };
        let candidates = manifest
            .config
            .runners
            .as_ref()
            .and_then(|runners| runners.get(role));
        if !candidates.into_iter().flatten().any(|c| c.adapter == *pin) {
            return Err(ServableError::NoCandidateOnPin {
                node: node.id.clone(),
                runner: role.clone(),
                pin: pin.clone(),
                candidates: candidates
                    .into_iter()
                    .flatten()
                    .map(|candidate| candidate.adapter.clone())
                    .collect(),
            }
            .into());
        }
    }
    Ok(())
}

/// Each frozen child, read as its parent froze it, under the name its `use:`
/// gives it. The name is the path the child is read by.
fn frozen_children(manifest: &Manifest) -> Result<Vec<(&str, Workflow)>, CliError> {
    manifest
        .frozen
        .iter()
        .flat_map(|frozen| &frozen.workflows)
        .map(|(name, child)| {
            let workflow = yunta_core::workflow::read::read(&child.text, Path::new(name))
                .map_err(|report| CliError::msg(report.to_string()))?;
            Ok((name.as_str(), workflow))
        })
        .collect()
}

/// The nodes the run reaches: the workflow's own, then each frozen child's.
fn reached_nodes<'a>(manifest: &'a Manifest, children: &'a [(&str, Workflow)]) -> Vec<&'a Node> {
    manifest
        .workflow
        .iter_nodes()
        .chain(children.iter().flat_map(|(_, child)| child.iter_nodes()))
        .collect()
}

/// What `check` says of the root and of each frozen child, judged against the
/// adapters this binary builds: the capability rules the run's declarations
/// must still meet. A child is judged with the mounts its `use:` nodes carry
/// in, as its own birth judges it.
fn refuse_unchecked(
    manifest: &Manifest,
    children: &[(&str, Workflow)],
    nodes: &[&Node],
) -> Result<(), CliError> {
    let declared = super::declared_facts(&manifest.config);
    let mut errors = yunta_engine::check(&manifest.workflow, &manifest.config, &declared);
    for (name, child) in children {
        let mounts = mounts_into(name, nodes);
        errors.extend(yunta_engine::check_mounted(
            child,
            &manifest.config,
            &declared,
            &mounts,
        ));
    }
    if errors.is_empty() {
        return Ok(());
    }
    Err(CliError::msg(yunta_core::text::problems(
        "the workflow fails `yunta check`",
        &errors,
    )))
}

/// Every mount the `use:` nodes of `name` carry in, across the nodes the run
/// reaches. A child used at several places is judged with the mounts of all
/// of them, so a read one use answers is not refused for another's sake.
fn mounts_into(name: &str, nodes: &[&Node]) -> Vec<MountSpec> {
    nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::Workflow { r#use, mounts, .. } if r#use.as_str() == name => Some(mounts),
            _ => None,
        })
        .flatten()
        .cloned()
        .collect()
}

/// The run's own directories, where a scripted fixture is rendered.
pub(crate) struct ScriptedPaths<'a> {
    pub(crate) run_dir: &'a Path,
    pub(crate) worktree: &'a Path,
}

/// The registry the manifest's sessions run on, built once per invocation.
pub(crate) fn adapters_for(
    manifest: &Manifest,
    paths: &ScriptedPaths<'_>,
) -> Result<Adapters, CliError> {
    adapters_and_script(manifest, paths).map(|(adapters, _)| adapters)
}

/// [`adapters_for`], with the scripted mock a mock run's registry is built
/// on, so a caller can ask which of its scripts no session claimed.
pub(crate) fn adapters_and_script(
    manifest: &Manifest,
    paths: &ScriptedPaths<'_>,
) -> Result<(Adapters, Option<Arc<MockAdapter>>), CliError> {
    match &manifest.adapter {
        Some(RunAdapter::Mock { fixture }) => {
            // The claims live in the run's directory, so a resumed run goes on
            // from the scripts its earlier sessions claimed.
            let claims = paths.run_dir.join(yunta_adapters::MOCK_CLAIMS_FILE);
            // The fixture error names the claims file it refused.
            let mock = Arc::new(
                parse_fixture(fixture, paths)?
                    .keeping_claims(claims)
                    .map_err(|source| CliError::msg(source.to_string()))?,
            );
            let adapters = yunta_adapters::registry(&manifest.config, Arc::clone(&mock));
            Ok((adapters, Some(mock)))
        }
        _ => Ok((super::real_adapters(&manifest.config), None)),
    }
}

/// Parses a fixture's text against a run's own directories: what a scripted
/// run takes its sessions from.
pub(crate) fn parse_fixture(
    text: &str,
    paths: &ScriptedPaths<'_>,
) -> Result<MockAdapter, CliError> {
    parse_script(text, paths)
        .map(MockAdapter::new)
        .map_err(|source| CliError::msg(format!("the run's fixture does not parse: {source}")))
}

/// Reads a `--fixture` file for a run that has not been created. The file is
/// parsed here too, so one that does not parse refuses the run before it is
/// created, and the refusal names the file.
///
/// The run's directories are unknown until the run exists, and each
/// placeholder renders to `.` meanwhile: a plain path, which leaves the
/// fixture's shape as a real run's directories leave it.
pub(crate) async fn read_fixture(path: &Path) -> Result<String, CliError> {
    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|source| CliError::io("read fixture", path.display(), source))?;
    let placeholder = Path::new(".");
    parse_script(
        &text,
        &ScriptedPaths {
            run_dir: placeholder,
            worktree: placeholder,
        },
    )
    .map_err(|source| CliError::FixtureRefused {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(text)
}

/// A fixture's text rendered against the run's directories, and parsed.
fn parse_script(text: &str, paths: &ScriptedPaths<'_>) -> Result<MockFixture, FixtureError> {
    let staging = yunta_engine::run_dir::staging_root(paths.run_dir);
    MockFixture::parse(
        text,
        &RunPaths {
            run_dir: paths.run_dir,
            worktree: paths.worktree,
            staging: &staging,
        },
    )
}

/// One run, handed to the engine.
///
/// The run an invocation was given and every successor a promotion makes
/// share this shape: the retry ceiling, the ambient environment, the
/// secrets source, and how a `RunEnv` is filled in. One literal writes all
/// of them, so a field added to a run is added to its successors too.
pub(crate) struct Executing<'a> {
    pub(crate) run_id: &'a RunId,
    pub(crate) manifest: &'a Manifest,
    pub(crate) run_dir: &'a Path,
    pub(crate) worktree: &'a Path,
    pub(crate) adapters: &'a Adapters,
    pub(crate) storage: &'a AsyncStorage,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: &'a dyn yunta_core::IdSource,
    pub(crate) human_interaction: &'a dyn yunta_engine::HumanInteraction,
    pub(crate) forge: Option<&'a dyn yunta_core::port::Forge>,
    pub(crate) cancel: &'a tokio_util::sync::CancellationToken,
    pub(crate) observer: Option<Arc<dyn yunta_engine::RunObserver>>,
    pub(crate) rail_command: yunta_core::rail::RailCommand,
    pub(crate) ambient: &'a yunta_core::Env,
}

/// Runs one run to its stop — the one `execute_run` call this binary
/// makes.
pub(crate) async fn execute(on: Executing<'_>) -> Result<RunReport, RunError> {
    yunta_engine::execute_run(RunEnv {
        run_id: on.run_id,
        manifest: on.manifest,
        run_dir: on.run_dir,
        worktree: on.worktree,
        adapters: on.adapters,
        storage: on.storage,
        clock: on.clock,
        ids: on.ids,
        human_interaction: on.human_interaction,
        forge: on.forge,
        cancel: on.cancel,
        ambient: Some(on.ambient),
        secrets: Arc::new(yunta_core::ProcessSecrets),
        observer: on.observer,
        rail_command: Some(on.rail_command),
    })
    .await
}

/// Creates a run on disk from its frozen manifest and documents, in the
/// checkout it works in. The one call from a command path into the
/// engine's run creation, so a run a person starts and a run a case plays
/// are born by the same step.
pub(crate) async fn create_frozen_run(
    ctx: &Context,
    storage: &AsyncStorage,
    frozen: &FrozenRun,
    run_id: &RunId,
    mode: &ModeName,
    worktree: &Path,
    redactor: &Redactor,
) -> Result<PathBuf, RunError> {
    yunta_engine::create_run(
        CreateRunParams {
            run_id,
            manifest: &frozen.manifest,
            runs_root: &ctx.project.runs_root,
            mode,
            worktree,
            promoted_from: None,
            // The run is born holding every document its `inputs:`
            // named, accepted right after `run_created`.
            artifacts: &frozen.documents,
            blobs: &frozen.blobs,
            // A run a caller starts is the root of its lineage: it
            // measures on its first wake, if its config names a suite.
            baseline: None,
            environment: yunta_engine::process::execution_environment(Some(&ctx.env)),
            redactor,
        },
        storage,
        ctx.supervision(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixture that is not YAML at all is refused at birth, and the refusal
    /// names the file, so the person knows which fixture to fix.
    #[test]
    fn a_fixture_that_does_not_parse_is_refused_naming_its_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("broken.yaml");
        std::fs::write(&path, "sessions: [unclosed\n").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime
            .block_on(read_fixture(&path))
            .expect_err("a fixture that is not YAML is refused");
        assert!(error.to_string().contains("broken.yaml"), "{error}");
    }

    /// The refusal offers the adapters `runners:` names that this binary
    /// builds, and says so in words of its own when there are none.
    #[test]
    fn an_unbuilt_pin_names_what_this_binary_can_run() {
        let refused = |runnable: Vec<AdapterId>| {
            ServableError::UnbuiltPin {
                pin: AdapterId::from("codex"),
                runnable,
            }
            .to_string()
        };
        assert_eq!(
            refused(vec![AdapterId::from("claude-code")]),
            "unknown adapter `codex` — this binary can run: claude-code"
        );
        assert_eq!(
            refused(Vec::new()),
            "unknown adapter `codex` — this binary can run: (none configured — `runners:` names no adapter this build supports)"
        );
    }
}
