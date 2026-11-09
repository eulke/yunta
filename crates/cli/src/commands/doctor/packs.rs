//! What `doctor` finds of the packs installed here: what each manifest
//! requires that this project lacks, and what each pack's workflows would
//! be refused for, each with what to declare when that is known.

use surco::components::{Check, Checklist, Fix, Found};
use yunta_core::{Detected, MachineAdapters};
use yunta_render::{caution, declare};

use crate::context::Context;
use crate::error::CliError;

/// Checks every installed pack's own `requires:` against this
/// project's merged config — roles resolvable, `mcp_servers:` defined,
/// and `requires.programs` present on `PATH` — a check per gap, each
/// unresolvable role with the runner to declare on one of `answered`, the
/// adapters that answered here. A project with no packs installed has
/// nothing to check.
pub(super) fn check_requires(ctx: &Context, answered: &MachineAdapters, checks: &mut Checklist) {
    for publisher in yunta_engine::installed_publishers(&ctx.cwd) {
        let packs = yunta_engine::packs_for_publisher(&ctx.cwd, &publisher);
        // A broken pack is a real gap: its manifest is the only place its
        // requirements are declared, so it is named, never skipped.
        for err in &packs.broken {
            checks.push(Found::Problem, "pack", err.to_string());
        }
        for (_, manifest) in packs.installed {
            let gap = yunta_engine::check_pack_requires(&manifest, &ctx.project.config);
            check_gap(&ctx.env, &gap, answered, checks);
        }
    }
}

/// The rows an installed pack's unmet `requires:` make, each runner it
/// lacks with the runner to declare on one of `answered`.
fn check_gap(
    env: &yunta_core::Env,
    gap: &yunta_engine::PackRequiresGap,
    answered: &MachineAdapters,
    checks: &mut Checklist,
) {
    let pack = format!("pack {}", gap.pack);
    for runner in &gap.missing_runners {
        push_once(
            checks,
            Check::new(
                Found::Problem,
                pack.as_str(),
                format!(
                    "requires runner `{runner}`, which `runners:` does not define, or \
                     defines with zero candidates"
                ),
            )
            .fix(declare::runners(
                std::slice::from_ref(runner),
                false,
                answered,
            )),
        );
    }
    for runner in &gap.retired_permissions {
        checks.push(
            Found::Problem,
            pack.as_str(),
            yunta_engine::retired_permissions_sentence(runner),
        );
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
        .filter(|program| !env.finds(program))
    {
        checks.push(
            Found::Problem,
            pack.as_str(),
            format!("requires program `{program}`, not found on PATH"),
        );
    }
}

/// Checks every installed pack's workflows as `yunta run` would check
/// them in this project, before anyone starts one: against this
/// project's config — a key a node cannot run without, a comparison
/// with no suite to measure — and with every `files:` path they read in
/// the commit this repository is on, the part of a pack's needs no
/// manifest declares, because only the workflow says it. What the manifest
/// `requires:` and a broken pack are [`check_requires`]'s to name.
pub(super) async fn check_workflows(
    ctx: &Context,
    answered: &MachineAdapters,
    checks: &mut Checklist,
) -> Result<(), CliError> {
    let config = &ctx.project.config;
    let isolation = config.resolved_isolation();
    let facts = yunta_api::machine::declared_facts(config);
    let detected = Detected {
        adapters: answered.clone(),
        ..yunta_api::machine::in_repo(&ctx.project.workspace, &ctx.cwd, &ctx.stop()).await?
    };
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
                let refused = yunta_engine::check(&workflow, config, &facts)
                    .into_iter()
                    .chain(
                        yunta_engine::check_workflow_refs(
                            &workflow, config, &ctx.cwd, &origin, &facts,
                        )
                        .errors,
                    );
                let found = ctx
                    .project
                    .context_files_at_head(&workflow, isolation, &ctx.stop())
                    .await?;
                let errors: Vec<yunta_core::check::CheckError> =
                    refused.chain(found.errors).collect();
                let pack = format!("pack {}", manifest.reference());
                for error in &errors {
                    push_once(checks, broken(error, pack.as_str(), &detected));
                }
                for warning in &found.cautions {
                    checks.push(Found::Caution, pack.as_str(), caution::line(warning));
                }
            }
        }
    }
    Ok(())
}

/// The row a pack's workflow is refused for: the fact, and what fixes it,
/// what this repository answers for it when it does, or what to declare.
fn broken(error: &yunta_core::check::CheckError, pack: &str, detected: &Detected) -> Check {
    let (said, remedy) = match yunta_render::refusal::check::lacking(error) {
        Some((fact, remedy)) => (fact, Some(Fix::said(remedy))),
        None => (yunta_render::refusal::check::line(error), None),
    };
    Check {
        fix: yunta_api::machine::fix_for(error, detected).or(remedy),
        ..Check::new(Found::Problem, pack, said)
    }
}

/// `check` on `checks`, with its fix only when no check already there
/// gives the same one: a fix is said once, under the first row that needs
/// it.
fn push_once(checks: &mut Checklist, check: Check) {
    let given = check.fix.as_ref().is_some_and(|fix| {
        checks
            .checks()
            .any(|earlier| earlier.fix.as_ref() == Some(fix))
    });
    checks.push_check(match given {
        true => Check { fix: None, ..check },
        false => check,
    });
}

#[cfg(test)]
mod tests {
    use surco::components::{Check, Checklist, Fix, Found};

    use super::push_once;

    #[test]
    fn a_fix_is_said_once_under_the_first_row_that_needs_it() {
        let fix = Fix::said("declare a runner in .yunta/config.yaml:");
        let mut checks = Checklist::default();
        for node in ["plan", "spec"] {
            let said = format!("node `{node}` references runner `planner`");
            push_once(
                &mut checks,
                Check::new(Found::Problem, "pack acme/review", said).fix(fix.clone()),
            );
        }
        let fixes: Vec<bool> = checks.checks().map(|check| check.fix.is_some()).collect();
        assert_eq!(fixes, [true, false]);
    }
}
