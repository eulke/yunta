//! `yunta run <workflow>`: resolve config, check, freeze the manifest,
//! prepare the run's isolated working tree and create the run.
//!
//! A run takes one of two shapes from there, one module each —
//! [`attached`] drives it here, [`detach`] hands it to a child that
//! outlives this process — and every step both of them share lives in
//! this module, so the two reach a run through the very same resolution,
//! refusals and creation.
//!
//! The run id is a ULID from the shell's id source; the engine mints
//! only the ids of the runs this one gives birth to, through the same
//! injected source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use yunta_adapters::MOCK_ID;
use yunta_core::{AdapterId, Clock, IdSource, Isolation, Manifest, ModeName, Workflow};
use yunta_engine::PriorEstimation;
use yunta_storage::AsyncStorage;

mod attached;
mod detach;

pub(crate) use detach::start_detached;

use super::drive::Prepared;
use super::Adapters;
use crate::context::Context;
use crate::error::{warn, CliError, Outcome};
use crate::load_yaml;

/// Parses `--input name=value` entries into the raw map
/// `yunta_engine::resolve_inputs` validates against the workflow's own
/// `inputs:` — this function only enforces the *syntax* of the flag
/// (exactly one `=`, non-empty name); everything about whether
/// a name is declared, required, or well-typed is `resolve_inputs`'s
/// job, not this one's, so the two error paths never disagree about who
/// owns which rule.
fn parse_inputs(raw: &[String]) -> Result<HashMap<String, String>, String> {
    let mut inputs = HashMap::new();
    for entry in raw {
        let (name, value) = entry
            .split_once('=')
            .ok_or_else(|| format!("--input `{entry}` must have the form `name=value`"))?;
        if name.is_empty() {
            return Err(format!("--input `{entry}` has an empty name"));
        }
        if inputs.insert(name.to_string(), value.to_string()).is_some() {
            return Err(format!("--input `{name}` was given more than once"));
        }
    }
    Ok(inputs)
}

/// `--adapter <name>` names a real adapter every session runs on; it
/// must be one `real_adapters` constructed from the config. `mock` is
/// handled before this: it needs a fixture, never a real binary.
fn validate_adapter_flag(name: &AdapterId, adapters: &Adapters) -> Result<(), String> {
    if !adapters.contains_key(name) {
        return Err(format!(
            "unknown adapter `{name}` — this binary can run: {}",
            if adapters.is_empty() {
                "(none configured — `runners:` names no adapter this build supports)".to_string()
            } else {
                let mut names: Vec<&str> = adapters.keys().map(AdapterId::as_str).collect();
                names.sort();
                names.join(", ")
            }
        ));
    }
    Ok(())
}

/// Every run in storage with events but no `run_finished` — paused runs
/// hold a slot (they expect a `resume`), finished ones never do.
async fn count_non_terminal_runs(
    storage: &AsyncStorage,
) -> Result<usize, yunta_storage::StorageError> {
    let mut active = 0;
    for run_id in storage.list_runs().await?.into_iter().map(|run| run.run_id) {
        let events = storage.events_for_run(run_id).await?;
        let finished = events.iter().any(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::RunFinished(_))
            )
        });
        if !events.is_empty() && !finished {
            active += 1;
        }
    }
    Ok(active)
}

/// Creates a run from a workflow and takes one of the two shapes a run
/// reaches a person in: driven here, with this invocation watching it to
/// its end, or handed to a detached child that outlives this process.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    fixture: Option<&Path>,
    mode: Option<&ModeName>,
    quiet: bool,
    detach: bool,
    json: bool,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.async_storage().await?;
    let mock_fixture = mock_fixture(adapter, fixture, detach)?;

    if detach {
        return detach::detached(detach::Detaching {
            ctx: &ctx,
            storage: &storage,
            workflow_path,
            raw_inputs,
            adapter,
            mode,
            quiet,
            json,
        })
        .await;
    }

    attached::attached(attached::Attaching {
        ctx: &ctx,
        storage: &storage,
        workflow_path,
        raw_inputs,
        adapter,
        mock_fixture,
        mode,
        quiet,
        json,
    })
    .await
}

/// `--adapter mock --fixture <path>`: the scripted fixture every session
/// runs against, with no real adapter constructed or probed.
///
/// The one place the flags around a mock run mean something: each
/// combination is either that fixture or a refusal naming what to do
/// instead, read before the workflow is even loaded so a misuse costs
/// nothing.
fn mock_fixture<'a>(
    adapter: Option<&AdapterId>,
    fixture: Option<&'a Path>,
    detach: bool,
) -> Result<Option<&'a Path>, CliError> {
    match (adapter, fixture) {
        // A fixture is read by whoever runs the sessions, and `--detach`
        // makes that a separate `yunta resume` — which resolves the
        // adapters `runners:` names and takes no fixture of its own.
        (Some(id), _) if *id == MOCK_ID && detach => Err(CliError::msg(
            "`--adapter mock` scripts every session from a fixture, and `--detach` hands \
             the run to a separate `yunta resume` that resolves the adapters `runners:` \
             names and reads no fixture — drop `--detach` to run against a fixture here, \
             or write a case and run `yunta test`",
        )),
        (Some(id), Some(path)) if *id == MOCK_ID => Ok(Some(path)),
        (Some(id), None) if *id == MOCK_ID => Err(CliError::msg(
            "`--adapter mock` runs the workflow against a scripted fixture — pass \
             `--fixture <path>` (the format a `.yunta/tests/` fixture uses), or write a \
             case and run `yunta test`",
        )),
        (_, Some(_)) => Err(CliError::msg(
            "`--fixture` only applies together with `--adapter mock`",
        )),
        _ => Ok(None),
    }
}

/// Everything a run settles before it exists: the workflow resolved and
/// checked, every adapter it names present and healthy, and the manifest
/// frozen — so what this refuses costs nothing, and what it returns is
/// the very run the caller is about to create.
///
/// Both shapes of a run come through here, which is what makes them
/// refuse the same workflow in the same words. A mock fixture skips the
/// adapter step whole: a scripted session needs no binary, so none is
/// built or probed for it, and the caller gets an empty registry to
/// fill from the fixture once the run has a directory to read it into.
async fn runnable(
    ctx: &Context,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    mock_fixture: Option<&Path>,
) -> Result<(Manifest, Adapters), CliError> {
    let (workflow_path, workflow) = resolve_and_check(ctx, workflow_path)?;
    let adapters = match mock_fixture {
        Some(_) => HashMap::new(),
        None => runnable_adapters(ctx, &workflow, adapter).await?,
    };
    let manifest = build_frozen_manifest(ctx, &workflow, &workflow_path, raw_inputs)?;
    Ok((manifest, adapters))
}

/// The real adapters this workflow can run on, refused before a worktree
/// or a token is spent if one is missing, unnamed or unhealthy.
///
/// Every path that is about to execute a workflow on real binaries goes
/// through here — the one that drives the run itself and the one that
/// hands it to a detached child — so a workflow no adapter can run is
/// refused in the same words whichever of them the caller asked for.
async fn runnable_adapters(
    ctx: &Context,
    workflow: &Workflow,
    adapter: Option<&AdapterId>,
) -> Result<Adapters, CliError> {
    let adapters = super::real_adapters(&ctx.project.config);
    super::refuse_unrunnable(workflow, &adapters)?;
    if let Some(name) = adapter {
        validate_adapter_flag(name, &adapters).map_err(CliError::msg)?;
    }
    super::probe_or_refuse(&adapters).await?;
    Ok(adapters)
}

/// What reading a workflow's history said before its run started.
///
/// The warning is kept rather than only printed: stderr reaches the
/// person watching, and §8.6 of the run contract makes this the one
/// piece of the estimation that is actionable — so it also reaches the
/// reader who has only a document, which is the reader most likely to
/// be automating the spend.
pub(super) struct Estimated {
    pub(super) prior: Option<PriorEstimation>,
    pub(super) budget_warning: Option<String>,
}

/// What this workflow's past runs cost, shown before anything is spent
/// and returned for the closing block's own comparison when the
/// invocation stays to draw one.
///
/// The distribution is informative, and this is where the two flags that
/// suppress it are read: whoever asked for the run id alone (`--quiet`)
/// or for a JSON document (`--json`) did not ask for context. The budget
/// warning survives both — it asks for a decision before tokens are
/// spent, and a run that stops halfway on a badly chosen cap is the most
/// expensive waste there is.
async fn estimate(
    ctx: &Context,
    storage: &AsyncStorage,
    manifest: &Manifest,
    quiet: bool,
    json: bool,
) -> Estimated {
    let history = {
        let runs_root = ctx.project.runs_root.clone();
        let workflow_name = manifest.workflow.name.clone();
        storage
            .blocking("collect the workflow's history", move |storage| {
                Ok(super::stats::collect_history(
                    &runs_root,
                    storage,
                    &workflow_name,
                ))
            })
            .await
            .unwrap_or_default()
    };
    let estimation = yunta_engine::prior_estimation(&history);
    if let Some(estimation) = &estimation {
        if !(quiet || json) {
            println!("{}", super::stats::format_estimation_line(estimation));
        }
    }
    let budget_warning = yunta_engine::budget_p90_warning(
        manifest
            .config
            .limits
            .as_ref()
            .and_then(|limits| limits.max_tokens_per_run),
        estimation.as_ref(),
    );
    if let Some(warning) = &budget_warning {
        warn(warning);
    }
    Estimated {
        prior: estimation,
        budget_warning,
    }
}

/// A workflow reference resolved to its file and loaded, refused if it
/// fails `yunta check` — the shared front of `yunta run` and the control
/// plane's `run_workflow`, so both reach the same workflow the same way: a
/// bare catalog name (no extension) resolves through the repo catalog then
/// a publisher's vendored packs (`acme/review`); anything with an
/// extension is taken as a literal path.
fn resolve_and_check(ctx: &Context, workflow_path: &Path) -> Result<(PathBuf, Workflow), CliError> {
    let resolved = super::resolve_workflow_ref(&ctx.cwd, workflow_path)?;
    let workflow: Workflow = load_yaml(&resolved, "workflow")?;
    super::check_or_refuse(&workflow, &ctx.project.config, &resolved)?;
    Ok((resolved, workflow))
}

/// Builds the run's manifest and freezes its state roots, which
/// [`FrozenPaths::new`](yunta_core::FrozenPaths::new) requires to be
/// absolute — `resume`/`status`/`gc` read these back from any directory,
/// so a relative root (a relative `paths.*` or `YUNTA_HOME`) is refused
/// here, naming it, rather than silently rooted at the invocation's cwd.
fn build_frozen_manifest(
    ctx: &Context,
    workflow: &Workflow,
    workflow_path: &Path,
    raw_inputs: &[String],
) -> Result<Manifest, CliError> {
    let provided_inputs = parse_inputs(raw_inputs).map_err(CliError::msg)?;
    let workflow_dir = workflow_path.parent().unwrap_or(Path::new("."));
    let mut manifest = yunta_engine::build_manifest(
        workflow,
        &ctx.project.config,
        workflow_dir,
        &ctx.cwd,
        &provided_inputs,
    )?;
    manifest.paths = Some(yunta_core::FrozenPaths::new(
        ctx.project.runs_root.clone(),
        ctx.project.worktrees_root.clone(),
    )?);
    Ok(manifest)
}

/// Enforces the soft concurrency cap, mints the run id from the injected
/// clock, prepares the isolation worktree and creates the run — the
/// shared create step of an executed run and a detached one. Prints
/// nothing of its own: the caller reports what it made.
async fn create_run_from(
    ctx: &Context,
    storage: &AsyncStorage,
    manifest: &Manifest,
    mode: Option<&ModeName>,
) -> Result<Prepared, CliError> {
    // A soft budget, not a safety limit — best-effort by design (two
    // simultaneous `yunta run` invocations can both pass the count),
    // checked before anything is created so the refusal costs nothing.
    if let Some(cap) = manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_concurrent_runs)
    {
        let active = count_non_terminal_runs(storage).await?;
        if active >= cap as usize {
            return Err(CliError::msg(format!(
                "{active} run(s) are still active and `limits.max_concurrent_runs` \
                 is {cap} — resume or cancel one (`yunta list` names them) before starting \
                 another"
            )));
        }
    }

    let run_id = ctx.ids.mint_run_id(ctx.clock.now());
    let worktree = match manifest.isolation {
        Isolation::Worktree => ctx.project.worktrees_root.join(run_id.as_str()),
        Isolation::None => ctx.cwd.clone(),
    };
    match yunta_engine::prepare_worktree(
        &ctx.cwd,
        &worktree,
        &manifest.base_commit,
        &format!("yunta/{run_id}"),
        manifest.isolation,
    )
    .await?
    {
        yunta_engine::WorktreePrepared::Ready => {}
        yunta_engine::WorktreePrepared::StoleStaleLock { dead_pid } => {
            warn(format!(
                "this checkout's isolation lock belonged to a dead process \
                 (pid {dead_pid}) — taking it over"
            ));
        }
    }

    // An explicit `--mode` is used as given (`create_run` itself refuses
    // an unknown name); omitted with `modes:` declared defaults to the
    // *first* declared mode — promotion only ever escalates forward, so
    // starting at the floor is the one default that can never need walking
    // back. A workflow with no `modes:` at all keeps running everything,
    // unaffected.
    let resolved_mode = mode.cloned().unwrap_or_else(|| {
        manifest
            .workflow
            .modes
            .as_ref()
            .and_then(|modes| modes.keys().next())
            .cloned()
            .unwrap_or_default()
    });

    let run_dir = yunta_engine::create_run(
        yunta_engine::CreateRunParams {
            run_id: &run_id,
            manifest,
            runs_root: &ctx.project.runs_root,
            mode: &resolved_mode,
            promoted_from: None,
            artifacts: &[],
        },
        storage,
        &ctx.clock,
    )
    .await?;

    Ok(Prepared {
        run_id,
        run_dir,
        worktree,
    })
}
