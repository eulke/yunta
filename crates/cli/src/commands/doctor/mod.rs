//! `yunta doctor`: probes every adapter this project's `runners:`
//! names — binary present, version compatible, auth valid — and
//! reports each one, healthy or not. The same `probe()` a real
//! `yunta run`/`yunta resume` calls before spending anything (see
//! `commands::probe_or_refuse`); this command exists to run it on
//! demand and report every result instead of stopping at the first
//! failure.
//!
//! Says who git commits a run's work as, or that it cannot name anyone.
//!
//! Also validates every installed pack's own `requires:` against this
//! project's merged config: roles resolvable, `mcp_servers:` defined,
//! and — the one part `yunta_engine::check_pack_requires` deliberately
//! leaves to this command, since it needs real filesystem access —
//! `requires.programs` present on `PATH`.

mod session;

use std::collections::BTreeSet;

use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::render::blocks::{paint, Checklist, Found, Next};
use crate::render::{Look, INDENT};
use yunta_core::port::ProbeReport;
use yunta_core::AdapterId;

pub async fn doctor(session: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let mut checks = Checklist::default();
    // The configuration a check found missing, as the lines to paste.
    let mut steps = Vec::new();
    let healthy = probe_adapters(&ctx, &mut checks, &mut steps).await;
    match super::committer::committer(&ctx).await {
        Ok(who) => checks.push(Found::Holds, "git", format!("commits as {who}")),
        Err(why) => checks.push(Found::Problem, "git", why.to_string()),
    }
    checks.extend(super::forge::forge_checks(&ctx).await);
    check_installed_pack_requires(&ctx.cwd, &ctx.project.config, &mut checks, &mut steps);
    check_installed_pack_workflows(&ctx, &mut checks, &mut steps).await;
    if session {
        probe_sessions(&ctx, &healthy, &mut checks).await;
    }

    let look = Look::stdout();
    let mut out = paint(&[&checks], &look);
    if !steps.is_empty() {
        out.push('\n');
        for line in &steps {
            out.push_str(&format!("{INDENT}{line}\n"));
        }
    }
    if !session {
        let next = Next {
            steps: vec![(
                "yunta doctor --session".to_string(),
                "opens a session per binding, a prompt each",
            )],
        };
        out.push('\n');
        out.push_str(&paint(&[&next], &look));
    }
    print!("{out}");

    match checks.holds() {
        true => Ok(Outcome::Success),
        false => Ok(Outcome::Reported),
    }
}

/// Probes every adapter this project's `runners:` names, a check each.
/// Hands back the ones that answered healthy — the only ones a session
/// is worth opening on.
async fn probe_adapters(
    ctx: &Context,
    checks: &mut Checklist,
    steps: &mut Vec<String>,
) -> BTreeSet<AdapterId> {
    let adapters = ctx.adapters();
    if adapters.is_empty() {
        no_runner_declared(ctx, checks, steps).await;
        return BTreeSet::new();
    }
    let mut healthy = BTreeSet::new();
    let mut names: Vec<&AdapterId> = adapters.keys().collect();
    names.sort();
    for name in names {
        let Some(adapter) = adapters.get(name) else {
            continue;
        };
        match adapter.probe().await {
            Ok(ProbeReport::Healthy { version }) => {
                healthy.insert(name.clone());
                let said = version
                    .as_deref()
                    .map(|v| format!(" ({v})"))
                    .unwrap_or_default();
                checks.push(Found::Holds, name.as_str(), format!("healthy{said}"));
            }
            Ok(ProbeReport::Unhealthy { diagnostic }) => {
                checks.push(
                    Found::Problem,
                    name.as_str(),
                    format!("unhealthy — {diagnostic}"),
                );
            }
            Err(e) => checks.push(Found::Problem, name.as_str(), format!("unhealthy — {e}")),
        }
    }
    healthy
}

/// What `doctor` finds of a project whose `runners:` names no adapter
/// this build supports: that nothing has an adapter to run on, and the
/// runner to declare with the adapters this machine answers for. A
/// problem when a workflow in the catalog needs one — it would stop the
/// first time it reached an agent node — and a caution otherwise.
async fn no_runner_declared(ctx: &Context, checks: &mut Checklist, steps: &mut Vec<String>) {
    let needing = needing_a_runner(ctx);
    let errors: Vec<yunta_engine::CheckError> = needing
        .iter()
        .flat_map(|(_, errors)| errors.iter().cloned())
        .collect();
    let detected = crate::detect::Detected::default().for_errors(&errors).await;
    let mut step = crate::detect::suggestions(&errors, &detected);
    if step.is_empty() {
        let probed = crate::detect::probe_known_adapters().await;
        step = crate::detect::runner_step(&[], true, &crate::detect::healthy(&probed));
    }
    let names: Vec<&str> = needing.iter().map(|(name, _)| name.as_str()).collect();
    match names.is_empty() {
        true => checks.push(
            Found::Caution,
            "runners",
            "none declared — a workflow's agent nodes cannot run until one is",
        ),
        false => checks.push(
            Found::Problem,
            "runners",
            format!(
                "none declared, and {} {} one",
                yunta_core::text::listed(names.iter().copied()),
                yunta_core::text::agreeing(names.len(), "needs", "need")
            ),
        ),
    }
    steps.extend(step);
}

/// Every catalog workflow that names a runner the config lacks, with
/// what `check` says about each.
fn needing_a_runner(ctx: &Context) -> Vec<(String, Vec<yunta_engine::CheckError>)> {
    super::list::catalog_workflows(&ctx.cwd)
        .into_iter()
        .map(|(name, workflow)| {
            let errors = yunta_engine::check(
                &workflow,
                &ctx.project.config,
                &super::declared_capabilities,
            )
            .into_iter()
            .filter(names_a_runner)
            .collect();
            (name, errors)
        })
        .filter(|(_, errors): &(String, Vec<_>)| !errors.is_empty())
        .collect()
}

/// Whether `error` is about a runner the config lacks.
fn names_a_runner(error: &yunta_engine::CheckError) -> bool {
    matches!(
        error,
        yunta_engine::CheckError::UnknownRunner { .. }
            | yunta_engine::CheckError::RunnerHasNoCandidates { .. }
            | yunta_engine::CheckError::Unset {
                key: yunta_core::ConfigKey::Runner,
                ..
            }
    )
}

/// Checks every installed pack's own `requires:` against this
/// project's merged config — roles resolvable, `mcp_servers:` defined,
/// and `requires.programs` present on `PATH` — a check per gap, and the
/// runner each unresolvable role needs as a step. A project with no
/// packs installed has nothing to check.
fn check_installed_pack_requires(
    cwd: &std::path::Path,
    config: &yunta_core::ConfigLayer,
    checks: &mut Checklist,
    steps: &mut Vec<String>,
) {
    for publisher in yunta_engine::installed_publishers(cwd) {
        let packs = yunta_engine::packs_for_publisher(cwd, &publisher);
        // A broken pack is a real gap: its manifest is the only place its
        // requirements are declared, so it is named, never skipped.
        for err in &packs.broken {
            checks.push(Found::Problem, "pack", err.to_string());
        }
        for (_, manifest) in packs.installed {
            let gap = yunta_engine::check_pack_requires(&manifest, config);
            let pack = format!("pack {}", gap.pack);
            for runner in &gap.missing_runners {
                checks.push(
                    Found::Problem,
                    pack.as_str(),
                    format!(
                        "requires runner `{runner}`, which `runners:` does not define, or \
                         defines with zero candidates"
                    ),
                );
                steps.extend([
                    format!("declare runner `{runner}` in .yunta/config.yaml:"),
                    "    runners:".to_string(),
                    format!("      {runner}:"),
                    format!(
                        "        - {{ adapter: {}, model: <model> }}",
                        super::first_built_adapter()
                    ),
                ]);
            }
            for server in &gap.missing_mcp_servers {
                checks.push(
                    Found::Problem,
                    pack.as_str(),
                    format!("requires mcp_server `{server}`, not defined under `mcp_servers:`"),
                );
            }
            for program in gap
                .required_programs
                .iter()
                .filter(|program| !super::refusals::command_on_path(program))
            {
                checks.push(
                    Found::Problem,
                    pack.as_str(),
                    format!("requires program `{program}`, not found on PATH"),
                );
            }
        }
    }
}

/// Checks every installed pack's workflows as `yunta run` would check
/// them in this project, before anyone starts one: against this
/// project's config — a key a node cannot run without, a comparison
/// with no suite to measure — and with every `files:` path they read in
/// the commit this repository is on, the part of a pack's needs no
/// manifest declares, because only the workflow says it. Returns `false`
/// (and prints a line per problem) when any has one; what the manifest
/// `requires:` and a broken pack are `check_installed_pack_requires`'s
/// to name.
async fn check_installed_pack_workflows(
    ctx: &Context,
    checks: &mut Checklist,
    steps: &mut Vec<String>,
) {
    let config = &ctx.project.config;
    let isolation = config.resolved_isolation();
    let detected = crate::detect::Detected::in_repo(&ctx.cwd, ctx.supervision()).await;
    for publisher in yunta_engine::installed_publishers(&ctx.cwd) {
        for (pack_dir, manifest) in
            yunta_engine::packs_for_publisher(&ctx.cwd, &publisher).installed
        {
            for declared in &manifest.contents.workflows {
                let Ok(workflow) = crate::load_workflow(&pack_dir.join(declared)) else {
                    continue;
                };
                let origin = yunta_engine::WorkflowOrigin::Pack {
                    publisher: publisher.clone(),
                    pack_name: manifest.name.clone(),
                };
                let refused = yunta_engine::check(&workflow, config, &super::declared_capabilities)
                    .into_iter()
                    .chain(
                        yunta_engine::check_workflow_refs(
                            &workflow,
                            config,
                            &ctx.cwd,
                            &origin,
                            &super::declared_capabilities,
                        )
                        .errors,
                    );
                let found = super::context_files_at_head(ctx, &workflow, isolation).await;
                let errors: Vec<yunta_engine::CheckError> = refused.chain(found.errors).collect();
                let pack = format!("pack {}", manifest.reference());
                for error in &errors {
                    checks.push(Found::Problem, pack.as_str(), error.to_string());
                }
                for warning in &found.warnings {
                    checks.push(Found::Caution, pack.as_str(), warning.to_string());
                }
                steps.extend(crate::detect::suggestions(&errors, &detected));
            }
        }
    }
}

/// Opens one session per binding whose adapter probed healthy, a check
/// each.
///
/// Only the bindings whose adapter is already healthy: the rest have
/// nothing a session could add, and the checks above already name them.
async fn probe_sessions(ctx: &Context, healthy: &BTreeSet<AdapterId>, checks: &mut Checklist) {
    let bindings: Vec<session::Binding> = session::bindings(&ctx.project.config)
        .into_iter()
        .filter(|binding| healthy.contains(&binding.candidate.adapter))
        .collect();
    if bindings.is_empty() {
        checks.push(
            Found::Caution,
            "sessions",
            "no binding to open a session on",
        );
        return;
    }
    for binding in &bindings {
        checks.push_check(session::session_probe(ctx, binding).await.check());
    }
}
