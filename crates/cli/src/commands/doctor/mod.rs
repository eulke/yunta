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
use yunta_core::port::ProbeReport;
use yunta_core::AdapterId;

pub async fn doctor(session: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let (healthy, all_probed) = probe_adapters(&ctx).await;
    let mut all_well = all_probed;

    match super::committer::committer(&ctx).await {
        Ok(who) => println!("git: commits as {who}"),
        Err(why) => {
            all_well = false;
            println!("git: {why}");
        }
    }
    all_well &= super::forge::report_forge(&ctx).await;

    if !check_installed_pack_requires(&ctx.cwd, &ctx.project.config) {
        all_well = false;
    }
    if !check_installed_pack_workflows(&ctx).await {
        all_well = false;
    }

    if session {
        all_well &= probe_sessions(&ctx, &healthy).await;
    } else {
        println!(
            "no session opened — `doctor` says the binary is there, answers and authenticates; \
             `yunta doctor --session` opens one per binding and says whether a run's session \
             actually starts, at the cost of a prompt each"
        );
    }

    if all_well {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Reported)
    }
}

/// Probes every adapter this project's `runners:` names and prints each
/// result. Hands back the ones that answered healthy — the only ones a
/// session is worth opening on — and whether all of them did.
async fn probe_adapters(ctx: &Context) -> (BTreeSet<AdapterId>, bool) {
    let adapters = ctx.adapters();
    if adapters.is_empty() {
        println!(
            "no adapter to probe — `runners:` in the merged config names none this build \
             supports (built: {})",
            super::built_adapter_names()
        );
        return (BTreeSet::new(), true);
    }
    let mut healthy = BTreeSet::new();
    let mut all_healthy = true;
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
                println!("{name}: healthy{said}");
            }
            Ok(ProbeReport::Unhealthy { diagnostic }) => {
                all_healthy = false;
                println!("{name}: unhealthy — {diagnostic}");
            }
            Err(e) => {
                all_healthy = false;
                println!("{name}: unhealthy — {e}");
            }
        }
    }
    (healthy, all_healthy)
}

/// Checks every installed pack's own `requires:` against this
/// project's merged config — roles resolvable, `mcp_servers:` defined,
/// and `requires.programs` present on `PATH`. Returns `false` (and
/// prints an actionable line per gap) when any pack has something
/// unmet; a project with no packs installed prints nothing and returns
/// `true`.
fn check_installed_pack_requires(cwd: &std::path::Path, config: &yunta_core::ConfigLayer) -> bool {
    let mut all_satisfied = true;
    for publisher in yunta_engine::installed_publishers(cwd) {
        let packs = yunta_engine::packs_for_publisher(cwd, &publisher);
        // A broken pack is a real gap: its manifest is the only place its
        // requirements are declared, so it is named, never skipped.
        for err in &packs.broken {
            all_satisfied = false;
            println!("{err}");
        }
        for (_, manifest) in packs.installed {
            let gap = yunta_engine::check_pack_requires(&manifest, config);
            let missing_programs: Vec<&String> = gap
                .required_programs
                .iter()
                .filter(|program| !super::refusals::command_on_path(program))
                .collect();
            if gap.is_satisfied() && missing_programs.is_empty() {
                continue;
            }
            all_satisfied = false;
            println!("pack {} requires:", gap.pack);
            for runner in &gap.missing_runners {
                println!(
                    "  runner `{runner}` — not resolvable: `runners:` doesn't define it, or \
                     defines it with zero candidates; add e.g.:\n      runners:\n        \
                     {runner}:\n          - {{ adapter: {}, model: <model> }}",
                    super::first_built_adapter()
                );
            }
            for server in &gap.missing_mcp_servers {
                println!(
                    "  mcp_server `{server}` — not defined under `mcp_servers:`; add it there"
                );
            }
            for program in missing_programs {
                println!("  program `{program}` — not found on PATH");
            }
        }
    }
    all_satisfied
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
async fn check_installed_pack_workflows(ctx: &Context) -> bool {
    let config = &ctx.project.config;
    let isolation = config.resolved_isolation();
    let detected = crate::detect::Detected::in_repo(&ctx.cwd, ctx.supervision()).await;
    let mut all_present = true;
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
                let said = errors
                    .iter()
                    .map(ToString::to_string)
                    .chain(found.warnings.iter().map(ToString::to_string))
                    .chain(crate::detect::suggestions(&errors, &detected));
                for line in said {
                    all_present = false;
                    println!("pack {}: {line}", manifest.reference());
                }
            }
        }
    }
    all_present
}

/// Opens one session per binding whose adapter probed healthy, and
/// prints how each ended. Returns `false` when any of them did not open.
///
/// Only the bindings whose adapter is already healthy: the rest have
/// nothing a session could add, and the lines above already name them.
async fn probe_sessions(ctx: &Context, healthy: &BTreeSet<AdapterId>) -> bool {
    let bindings: Vec<session::Binding> = session::bindings(&ctx.project.config)
        .into_iter()
        .filter(|binding| healthy.contains(&binding.candidate.adapter))
        .collect();
    if bindings.is_empty() {
        println!("no binding to open a session on");
        return true;
    }
    let mut all_opened = true;
    for binding in &bindings {
        let probe = session::session_probe(ctx, binding).await;
        all_opened &= probe.is_ok();
        println!("{probe}");
    }
    all_opened
}
