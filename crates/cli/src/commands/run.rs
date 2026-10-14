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
use std::path::Path;

use yunta_adapters::MOCK_ID;
use yunta_core::caution::Caution;
use yunta_core::{AdapterId, Clock, IdSource, InputName, Manifest, ModeName, RunAdapter, Workflow};
use yunta_engine::{CheckError, FrozenRun, PriorEstimation};
use yunta_storage::AsyncStorage;

mod attached;
mod detach;
mod mode;

pub(crate) use detach::start_detached;

use super::drive::Prepared;
use super::registry::{read_fixture, servable};
use crate::context::Context;
use crate::error::{warn, CliError, Outcome};
use crate::json::PreRunWarnings;
use crate::output::Mode;
use crate::render::Look;
use mode::resolve_mode;

/// Parses `--input name=value` entries into the raw map
/// `yunta_engine::resolve_inputs` validates against the workflow's own
/// `inputs:` — this function only enforces the *syntax* of the flag
/// (exactly one `=`, non-empty name); everything about whether
/// a name is declared, required, or well-typed is `resolve_inputs`'s
/// job, not this one's, so the two error paths never disagree about who
/// owns which rule.
fn parse_inputs(raw: &[String]) -> Result<HashMap<InputName, String>, String> {
    let mut inputs = HashMap::new();
    for entry in raw {
        let (name, value) = entry
            .split_once('=')
            .ok_or_else(|| format!("--input `{entry}` must have the form `name=value`"))?;
        let name: InputName = name
            .parse()
            .map_err(|error| format!("--input `{entry}`: {error}"))?;
        if inputs.insert(name.clone(), value.to_string()).is_some() {
            return Err(format!("--input `{name}` was given more than once"));
        }
    }
    Ok(inputs)
}

/// Every run in storage that has events and has neither closed nor broken:
/// paused runs hold a slot (they expect a `resume`), finished and broken
/// runs never do.
async fn count_non_terminal_runs(
    storage: &AsyncStorage,
) -> Result<usize, yunta_storage::StorageError> {
    let mut active = 0;
    for run_id in storage.list_runs().await?.into_iter().map(|run| run.run_id) {
        let events = storage.events_for_run(run_id).await?;
        if !events.is_empty() && !yunta_engine::derive(&events).ended() {
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
    output: Mode,
    detach: bool,
    look: &Look,
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
            output,
            look,
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
        output,
        look,
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
/// checked, git able to commit its work, the manifest frozen with the
/// adapter it runs on pinned into it, and every adapter it names present
/// and healthy — so what this refuses costs nothing, and what it returns is
/// the very run the caller is about to create.
///
/// The pin is what `--adapter` names: a real adapter by its id, or a mock
/// fixture's text, read here so the run keeps it. Both shapes of a run come
/// through here, which is what makes them refuse the same workflow in the
/// same words. What the adapters step refuses is [`servable`]'s.
///
/// `ambient` is the environment the run is born in: the user knowledge it
/// freezes is read from there, the way its sessions read it at their nodes.
pub(super) async fn runnable(
    ctx: &Context,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    mock_fixture: Option<&Path>,
    ambient: &yunta_core::Env,
) -> Result<FrozenRun, CliError> {
    let (workflow_path, workflow) = super::verdict::resolve_runnable(ctx, workflow_path).await?;
    super::committer::refuse_without_committer(ctx).await?;
    if mock_fixture.is_none() {
        super::forge::refuse_unreachable_forge(
            &ctx.project.config,
            &workflow,
            &yunta_core::ProcessSecrets,
        )?;
    }
    let pinned = match mock_fixture {
        Some(path) => Some(RunAdapter::Mock {
            fixture: read_fixture(path).await?,
        }),
        None => adapter.map(|id| RunAdapter::Named(id.clone())),
    };
    let frozen = build_frozen_manifest(ctx, &workflow, &workflow_path, raw_inputs, ambient)
        .await?
        .with_adapter(pinned);
    servable(&frozen.manifest).await?;
    Ok(frozen)
}

/// What an invocation says about a run before it creates it: what this
/// workflow's history cost, and every warning a person can still act on
/// for free.
///
/// The warnings are kept rather than only printed: stderr reaches the
/// person watching, and they also have to reach the reader who has only
/// a document, which is the reader most likely to be automating the
/// spend.
pub(super) struct Preflight {
    pub(super) prior: Option<PriorEstimation>,
    pub(super) warnings: PreRunWarnings,
}

/// Everything said before the first token: the estimation, and the
/// `files:` paths the run would not find in the tree it starts from.
pub(super) async fn preflight(
    ctx: &Context,
    manifest: &Manifest,
    mode: Option<&ModeName>,
    output: Mode,
    look: &Look,
) -> Result<Preflight, CliError> {
    // A run started here is started fresh in its mode: what only a run
    // promoted into that mode would hold, it does not.
    let started_in = resolve_mode(manifest, mode);
    let refused = yunta_engine::check_mode_start(&manifest.workflow, &started_in);
    if !refused.is_empty() {
        return Err(check_refusal(&refused));
    }
    let context_files = context_files(ctx, manifest, mode).await?;
    let (prior, budget) = estimate(ctx, manifest, &started_in, output, look).await;
    // The document lists the budget first, then the files a node reads.
    let cautions = budget.into_iter().chain(context_files).collect();
    Ok(Preflight {
        prior,
        warnings: PreRunWarnings { cautions },
    })
}

/// The refusal a workflow's own check makes, each error in the words a
/// person reads.
fn check_refusal(errors: &[CheckError]) -> CliError {
    let words: Vec<String> = errors.iter().map(crate::error::words::check).collect();
    CliError::msg(yunta_core::text::problems(
        "the workflow fails `yunta check`",
        &words,
    ))
}

/// The literal `files:` paths the nodes of this run's mode read that the
/// commit it starts from does not hold — said now, before the nodes
/// ahead of them spend, which is when the run itself would find out. One
/// nothing before its reader can write refuses the run; one a node that
/// runs earlier might write is warned about.
async fn context_files(
    ctx: &Context,
    manifest: &Manifest,
    mode: Option<&ModeName>,
) -> Result<Vec<Caution>, CliError> {
    let mode = resolve_mode(manifest, mode);
    let left_out = yunta_core::left_out(&manifest.workflow, &manifest.config);
    let included = yunta_engine::included_nodes(&manifest.workflow, &mode, &left_out);
    let found = super::context_files(
        ctx,
        &manifest.workflow,
        included.as_ref(),
        manifest.isolation,
        &manifest.base_commit,
    )
    .await?;
    if !found.errors.is_empty() {
        return Err(check_refusal(&found.errors));
    }
    for warning in &found.warnings {
        warn(yunta_render::caution::line(warning));
    }
    Ok(found.warnings)
}

/// What this workflow's past runs cost, shown in advance of anything being
/// spent and returned for the closing block's own comparison when the
/// invocation stays to draw one.
///
/// The distribution is informative, so only a person's terminal in
/// `Human` mode is given it: whoever asked for the run id alone or for a
/// JSON document did not ask for context. The budget caution survives
/// every mode — it asks for a decision in advance of spending tokens, and
/// a run that stops halfway on a badly chosen cap is the most expensive
/// waste there is.
///
/// Only the runs that started in `mode` and carry this workflow's hash
/// are a sample of what this run costs: a run in another mode, or of
/// another shape of the workflow, would read as this run's own cost.
async fn estimate(
    ctx: &Context,
    manifest: &Manifest,
    mode: &ModeName,
    output: Mode,
    look: &Look,
) -> (Option<PriorEstimation>, Option<Caution>) {
    let history = super::stats::budget_sample(ctx, manifest, mode, None).await;
    let estimation = yunta_engine::prior_estimation(&history);
    if let Some(estimation) = &estimation {
        crate::output::progress(
            output,
            look,
            super::stats::format_estimation_line(estimation, look.glyphs),
        );
    }
    let budget = yunta_engine::budget_caution(
        manifest
            .config
            .limits
            .as_ref()
            .and_then(|limits| limits.max_tokens_per_run),
        estimation.as_ref(),
    );
    if let Some(caution) = &budget {
        warn(yunta_render::caution::line(caution));
    }
    (estimation, budget)
}

/// Freezes the run: its manifest, every input its nodes read, the
/// documents its `inputs:` named, and its state roots, which
/// [`FrozenPaths::new`](yunta_core::FrozenPaths::new) requires to be
/// absolute — `resume`/`status`/`gc` read these back from any directory,
/// so a relative root (a relative `paths.*` or `YUNTA_HOME`) is refused
/// here, naming it, rather than silently rooted at the invocation's cwd.
async fn build_frozen_manifest(
    ctx: &Context,
    workflow: &Workflow,
    workflow_path: &Path,
    raw_inputs: &[String],
    ambient: &yunta_core::Env,
) -> Result<FrozenRun, CliError> {
    let provided_inputs = parse_inputs(raw_inputs).map_err(CliError::msg)?;
    let workflow_dir = workflow_path.parent().unwrap_or(Path::new("."));
    // The user layer's knowledge, where the environment keeps it.
    let user_knowledge = yunta_core::user_knowledge_dir(ambient);
    let mut frozen = yunta_engine::build_manifest(
        workflow,
        &ctx.project.config,
        workflow_dir,
        &ctx.cwd,
        &provided_inputs,
        ctx.supervision(),
        user_knowledge.as_deref(),
    )
    .await?;
    frozen.manifest.paths = Some(yunta_core::FrozenPaths::new(
        ctx.project.runs_root.clone(),
        ctx.project.worktrees_root.clone(),
    )?);
    Ok(frozen)
}

/// Enforces the soft concurrency cap, mints the run id from the injected
/// clock, prepares the isolation worktree and creates the run — the
/// shared create step of an executed run and a detached one. Prints
/// nothing of its own: the caller reports what it made.
pub(super) async fn create_run_from(
    ctx: &Context,
    storage: &AsyncStorage,
    frozen: &FrozenRun,
    mode: Option<&ModeName>,
) -> Result<Prepared, CliError> {
    let manifest = &frozen.manifest;
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
                "{} still active and `limits.max_concurrent_runs` is {cap} — resume or \
                 cancel one (`yunta list` names them) before starting another",
                yunta_core::text::counted(active, "run")
            )));
        }
    }

    // Resolved before anything is made: a short declared secret refuses the
    // run here, before its identifier is minted, a checkout is taken or a
    // branch is made.
    let redactor = yunta_engine::run_redactor(manifest, &yunta_core::ProcessSecrets)?;
    let run_id = ctx.ids.mint_run_id(ctx.clock.now());
    let worktree = match manifest.isolation {
        // A run of its own works in a checkout of the project's pool, on a
        // branch of its own: warm with whatever the last run there built,
        // or — for a run whose units build — the one holding the least.
        yunta_core::Isolation::Worktree => {
            let run_dir = ctx.project.runs_root.join(run_id.as_str());
            let pool =
                ctx.project
                    .pool_of(manifest, &ctx.cwd, &run_dir, std::sync::Arc::new(ctx.clock));
            let birth = yunta_engine::Birth::of(&manifest.workflow);
            let (checkout, _held) = pool
                .open_run(&run_id, &manifest.base_commit, birth, ctx.supervision())
                .await?;
            checkout
        }
        yunta_core::Isolation::None => {
            let worktree = ctx.project.run_tree(manifest, None, &run_id, &ctx.cwd);
            match yunta_engine::prepare_worktree(
                &ctx.cwd,
                &worktree,
                &manifest.base_commit,
                &yunta_engine::run_branch(&run_id),
                manifest.isolation,
                ctx.supervision(),
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
            worktree
        }
    };

    let resolved_mode = resolve_mode(manifest, mode);

    let run_dir = yunta_engine::create_run(
        yunta_engine::CreateRunParams {
            run_id: &run_id,
            manifest,
            runs_root: &ctx.project.runs_root,
            mode: &resolved_mode,
            worktree: &worktree,
            promoted_from: None,
            // The run is born holding every document its `inputs:`
            // named, accepted right after `run_created`.
            artifacts: &frozen.documents,
            blobs: &frozen.blobs,
            // A run a caller starts is the root of its lineage: it
            // measures on its first wake, if its config names a suite.
            baseline: None,
            environment: yunta_engine::process::execution_environment(Some(&ctx.env)),
            redactor: &redactor,
        },
        storage,
        ctx.supervision(),
    )
    .await?;

    Ok(Prepared {
        run_id,
        run_dir,
        worktree,
    })
}
