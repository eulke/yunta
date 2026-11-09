//! The registry a run's sessions run on, and what this binary refuses before
//! a run exists or a resume takes anything. Every invocation that drives a
//! run reaches its adapters through here: `run`, `resume`, a detached child,
//! a test case and a promotion successor each take the registry their
//! manifest names, so a run behaves the same whichever of them drives it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_adapters::{FixtureError, MockAdapter, MockFixture, RunPaths};
use yunta_core::{Clock, Manifest, RunAdapter, RunId};
use yunta_engine::{RunEnv, RunError, RunReport};
use yunta_storage::AsyncStorage;

use crate::context::Context;
use crate::error::CliError;
use yunta_api::machine::Adapters;

/// The run's own directories, where a scripted fixture is rendered.
pub(crate) struct ScriptedPaths<'a> {
    pub(crate) run_dir: &'a Path,
    pub(crate) worktree: &'a Path,
}

/// The registry the manifest's sessions run on, with the scripted mock a mock run's registry is built
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
        _ => Ok((yunta_api::machine::real_adapters(&manifest.config), None)),
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
    pub(crate) hold: yunta_engine::RunHold,
    pub(crate) worktree: &'a Path,
    pub(crate) adapters: &'a Adapters,
    pub(crate) storage: &'a AsyncStorage,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: &'a dyn yunta_core::IdSource,
    pub(crate) human_interaction: &'a dyn yunta_core::port::HumanInteraction,
    pub(crate) forge: Option<&'a dyn yunta_core::port::Forge>,
    pub(crate) cancel: &'a tokio_util::sync::CancellationToken,
    pub(crate) observer: Option<Arc<dyn yunta_core::port::RunObserver>>,
    pub(crate) rail_command: yunta_core::rail::RailCommand,
    pub(crate) ambient: &'a yunta_core::Env,
}

/// Claims a run nothing is leaving, one this invocation created or is taking on
/// again by itself: anyone else holding it is refused at once.
pub(crate) async fn claim(
    ctx: &Context,
    run_dir: &Path,
) -> Result<yunta_engine::RunHold, RunError> {
    let hold = yunta_engine::hold_run(
        run_dir,
        yunta_engine::lock::Contention::Refuse,
        ctx.clock.as_ref(),
        ctx.cancellation(),
    )
    .await?;
    Ok(hold)
}

/// Runs one run to its stop — the one `execute_run` call this binary
/// makes. The claim ends as this returns, before anything hands the run on: a
/// detached resume that waits for a reset would otherwise find its own parent
/// holding the run.
pub(crate) async fn execute(on: Executing<'_>) -> Result<RunReport, RunError> {
    yunta_engine::execute_run(RunEnv {
        run_id: on.run_id,
        manifest: on.manifest,
        hold: &on.hold,
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

/// A scripted run's identity and the paths it lives at, as the test runner
/// drives it.
pub(crate) struct Prepared {
    pub(crate) run_id: RunId,
    pub(crate) run_dir: PathBuf,
    pub(crate) worktree: PathBuf,
}

impl Prepared {
    /// The run the API created, as the test runner drives it.
    pub(crate) fn of(created: &yunta_api::CreatedRun<'_>) -> Self {
        Prepared {
            run_id: created.id.clone(),
            run_dir: created.dir.clone(),
            worktree: created.worktree.clone(),
        }
    }
}
