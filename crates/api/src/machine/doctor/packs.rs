//! See [`super`]. What `doctor` finds of the packs installed here: what each
//! manifest requires that this project lacks, and what each pack's workflows
//! would be refused for, each with what to declare when that is known.

use yunta_core::{Detected, MachineAdapters};
use yunta_render::declare;
use yunta_render::page::doctor::{Finding, PackLack};

use crate::{Error, Project, Stop};

/// Checks every installed pack's own `requires:` against this
/// project's merged config — roles resolvable, `mcp_servers:` defined,
/// and `requires.programs` present on `PATH` — a finding per gap, each
/// unresolvable role with the runner to declare on one of `answered`, the
/// adapters that answered here. A project with no packs installed has
/// nothing to check.
pub(super) fn check_requires(
    project: &Project,
    answered: &MachineAdapters,
    findings: &mut Vec<Finding>,
) {
    let repo = &project.workspace.dir;
    for publisher in yunta_engine::installed_publishers(repo) {
        let packs = yunta_engine::packs_for_publisher(repo, &publisher);
        // A broken pack is a real gap: its manifest is the only place its
        // requirements are declared, so it is named, never skipped.
        for err in &packs.broken {
            findings.push(Finding::PackBroken {
                said: err.to_string(),
            });
        }
        for (_, manifest) in packs.installed {
            let gap = yunta_engine::check_pack_requires(&manifest, &project.config);
            check_gap(&project.workspace.env, &gap, answered, findings);
        }
    }
}

/// What an installed pack's unmet `requires:` make, each runner it lacks
/// with the runner to declare on one of `answered`.
fn check_gap(
    env: &yunta_core::Env,
    gap: &yunta_engine::PackRequiresGap,
    answered: &MachineAdapters,
    findings: &mut Vec<Finding>,
) {
    let lacks = |lack| Finding::PackLacks {
        pack: gap.pack.clone(),
        lack,
    };
    for runner in &gap.missing_runners {
        let declare = declare::runners(std::slice::from_ref(runner), false, answered);
        findings.push(lacks(PackLack::Runner {
            runner: runner.clone(),
            declare,
        }));
    }
    for runner in &gap.retired_permissions {
        findings.push(lacks(PackLack::RetiredPermissions {
            said: yunta_engine::retired_permissions_sentence(runner),
        }));
    }
    for server in &gap.missing_mcp_servers {
        findings.push(lacks(PackLack::McpServer {
            server: server.to_string(),
        }));
    }
    for program in gap
        .required_programs
        .iter()
        .filter(|program| env.path.find(program).is_none())
    {
        findings.push(lacks(PackLack::Program {
            program: program.to_string(),
        }));
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
    project: &Project,
    answered: &MachineAdapters,
    findings: &mut Vec<Finding>,
    stop: &Stop,
) -> Result<(), Error> {
    let config = &project.config;
    let repo = &project.workspace.dir;
    let isolation = config.resolved_isolation();
    let facts = crate::machine::declared_facts(config);
    let detected = Detected {
        adapters: answered.clone(),
        ..crate::machine::in_repo(&project.workspace, repo, stop).await?
    };
    for publisher in yunta_engine::installed_publishers(repo) {
        for (pack_dir, manifest) in yunta_engine::packs_for_publisher(repo, &publisher).installed {
            for declared in &manifest.contents.workflows {
                let path = pack_dir.join(declared);
                let Ok(workflow) = crate::workflow::read(&path, &path) else {
                    continue;
                };
                let origin = yunta_engine::WorkflowOrigin::Pack {
                    publisher: publisher.clone(),
                    pack_name: manifest.name.clone(),
                };
                let refused = yunta_engine::check(&workflow, config, &facts)
                    .into_iter()
                    .chain(
                        yunta_engine::check_workflow_refs(&workflow, config, repo, &origin, &facts)
                            .errors,
                    );
                let found = project
                    .context_files_at_head(&workflow, isolation, stop)
                    .await?;
                let pack = manifest.reference();
                for error in refused.chain(found.errors) {
                    let fix = crate::machine::fix_for(&error, &detected);
                    findings.push(Finding::PackRefused {
                        pack: pack.clone(),
                        error,
                        fix,
                    });
                }
                for caution in found.cautions {
                    findings.push(Finding::PackCaution {
                        pack: pack.clone(),
                        caution,
                    });
                }
            }
        }
    }
    Ok(())
}
