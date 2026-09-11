//! `yunta run <workflow>`: resolve config, check, freeze the manifest,
//! prepare the run's isolated working tree, create the run and execute
//! it there.
//!
//! The run id is a ULID from the shell's id source; the engine mints
//! only the ids of the runs this one gives birth to, through the same
//! injected source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use yunta_adapters::MOCK_ID;
use yunta_core::{AdapterId, Clock, IdSource, Isolation, Manifest, ModeName, RunId, Workflow};
use yunta_engine::PriorEstimation;
use yunta_storage::AsyncStorage;

use super::drive::{drive, Driving, Prepared, RunJson};
use crate::context::Context;
use crate::error::{note, warn, CliError, Outcome};
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
fn validate_adapter_flag(
    name: &AdapterId,
    adapters: &HashMap<AdapterId, std::sync::Arc<dyn yunta_adapters::Adapter>>,
) -> Result<(), String> {
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
    let mock_fixture = mock_fixture(adapter, fixture)?;

    // Detach is the control plane's own shape: create the run, hand it to
    // a fully independent `yunta resume`, and return its id without
    // waiting — never with a mock fixture, which is an inline test tool.
    // The same create-and-hand-off the MCP `run_workflow` tool performs,
    // plus the estimation this invocation owes the person who typed it.
    if detach && mock_fixture.is_none() {
        return detached(Detaching {
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

    let (workflow_path, workflow) = resolve_and_check(&ctx, workflow_path)?;
    // A mock fixture needs no binary at all, so nothing is built or
    // probed for it.
    let real_adapters = match mock_fixture {
        Some(_) => HashMap::new(),
        None => runnable_adapters(&ctx, &workflow, adapter).await?,
    };
    let manifest = build_frozen_manifest(&ctx, &workflow, &workflow_path, raw_inputs)?;
    let prior = estimate(&ctx, &storage, &manifest, quiet, json).await;

    let prepared = create_run_from(&ctx, &storage, &manifest, mode).await?;
    if !json {
        println!(
            "run {}: created at {}",
            prepared.run_id,
            prepared.run_dir.display()
        );
    }

    let adapters = match mock_fixture {
        Some(path) => {
            let mock = super::test::load_mock_fixture(path, &prepared.run_dir, &prepared.worktree)
                .map_err(CliError::msg)?;
            super::test::mock_adapters(&ctx.project.config, mock)
        }
        None => real_adapters,
    };
    drive(Driving {
        ctx: &ctx,
        storage: &storage,
        manifest: &manifest,
        prepared: &prepared,
        adapters,
        adapter_override: adapter.filter(|id| **id != MOCK_ID).cloned(),
        prior,
        quiet,
        json,
    })
    .await
}

/// `--adapter mock --fixture <path>`: the scripted fixture every session
/// runs against, with no real adapter constructed or probed. Read before
/// the workflow is even loaded, so a misuse fails before anything is
/// resolved.
fn mock_fixture<'a>(
    adapter: Option<&AdapterId>,
    fixture: Option<&'a Path>,
) -> Result<Option<&'a Path>, CliError> {
    match (adapter, fixture) {
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
) -> Result<HashMap<AdapterId, std::sync::Arc<dyn yunta_adapters::Adapter>>, CliError> {
    let adapters = super::real_adapters(&ctx.project.config);
    super::refuse_unrunnable(workflow, &adapters)?;
    if let Some(name) = adapter {
        validate_adapter_flag(name, &adapters).map_err(CliError::msg)?;
    }
    super::probe_or_refuse(&adapters).await?;
    Ok(adapters)
}

/// What `yunta run --detach` needs to reach a run of its own: the same
/// workflow reference, inputs and overrides every run resolves, plus how
/// this invocation reports what it made.
struct Detaching<'a> {
    ctx: &'a Context,
    storage: &'a AsyncStorage,
    workflow_path: &'a Path,
    raw_inputs: &'a [String],
    adapter: Option<&'a AdapterId>,
    mode: Option<&'a ModeName>,
    /// `--quiet`: the run id and nothing else on stdout. The budget
    /// warning §8.6 of the run contract keeps actionable still goes out.
    quiet: bool,
    /// `--json`: one versioned document on stdout and nothing else.
    json: bool,
}

/// Creates the run, hands it to a detached `yunta resume`, and reports
/// its id without waiting on it.
///
/// The estimation happens here, between freezing the manifest and
/// creating the run, because this invocation is the only one that can
/// carry it: §8.6 of the run contract gives the distribution — and the
/// budget warning derived from it — to whoever *creates* a run, and a
/// `yunta resume`, detached or not, picks one up instead. A warning that
/// asks whether a cap is worth starting under is worth nothing once the
/// child is already spending, so it goes out before the child exists.
async fn detached(detaching: Detaching<'_>) -> Result<Outcome, CliError> {
    let Detaching {
        ctx,
        storage,
        workflow_path,
        raw_inputs,
        adapter,
        mode,
        quiet,
        json,
    } = detaching;
    let manifest = runnable_manifest(ctx, workflow_path, raw_inputs, adapter).await?;
    estimate(ctx, storage, &manifest, quiet, json).await;
    let run_id = create_and_detach(ctx, storage, &manifest, mode).await?;
    if json {
        return crate::json::print_json(&RunJson::detached(&run_id));
    }
    println!("run {run_id}: detached, driving forward independently");
    Ok(Outcome::Success)
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
) -> Option<PriorEstimation> {
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
    if let Some(warning) = yunta_engine::budget_p90_warning(
        manifest
            .config
            .limits
            .as_ref()
            .and_then(|limits| limits.max_tokens_per_run),
        estimation.as_ref(),
    ) {
        note(warning);
    }
    estimation
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

/// The frozen manifest of a real (non-mock) workflow that resolved,
/// passed `yunta check` and has every adapter it names, ready to run —
/// everything a detached start settles before a run exists, so what it
/// refuses costs nothing and what it returns is the very manifest the
/// detached child will execute.
async fn runnable_manifest(
    ctx: &Context,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
) -> Result<Manifest, CliError> {
    let (workflow_path, workflow) = resolve_and_check(ctx, workflow_path)?;
    runnable_adapters(ctx, &workflow, adapter).await?;
    build_frozen_manifest(ctx, &workflow, &workflow_path, raw_inputs)
}

/// Creates the run and hands it to a detached `yunta resume`, returning
/// its id without waiting on the child. Prints nothing: the caller
/// decides how to report the id (a line, or a DTO).
async fn create_and_detach(
    ctx: &Context,
    storage: &AsyncStorage,
    manifest: &Manifest,
    mode: Option<&ModeName>,
) -> Result<RunId, CliError> {
    let prepared = create_run_from(ctx, storage, manifest, mode).await?;
    super::spawn_detached_resume(&prepared.run_dir, prepared.run_id.as_str(), &ctx.cwd)
        .await
        .map_err(|source| {
            CliError::io(
                "spawn a detached",
                format!("`yunta resume {}`", prepared.run_id),
                source,
            )
        })?;
    Ok(prepared.run_id)
}

/// Creates a run for a real (non-mock) workflow and hands it to a detached
/// `yunta resume`, returning its id without waiting — the control plane's
/// `run_workflow`, resolving, checking, probing, freezing and creating
/// through the same [`runnable_manifest`] and [`create_and_detach`] pair
/// `yunta run --detach` uses, so both reach a run identically.
///
/// Says nothing about what this workflow has cost before: an agent client
/// reads the run id this returns, and §8.6 of the run contract hands that
/// client the same distribution through `list_workflows` — the surface it
/// consults while it is still choosing a workflow.
pub(crate) async fn start_detached(
    ctx: &Context,
    storage: &AsyncStorage,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    mode: Option<&ModeName>,
) -> Result<RunId, CliError> {
    let manifest = runnable_manifest(ctx, workflow_path, raw_inputs, adapter).await?;
    create_and_detach(ctx, storage, &manifest, mode).await
}
