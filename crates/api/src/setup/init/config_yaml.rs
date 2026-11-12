//! See [`super`]. The project's config as `init` writes it, YAML a person reads and
//! commits: its name and base branch, what the repository was detected to run, and its
//! runners, each declared as a person answered it or proposed, commented, beside the
//! adapters this machine answered for. Every value a person typed is written so it
//! reads back whole (`tests`).

use yunta_core::step::NextStep;
use yunta_core::{Detected, ProbedAdapter, RunnerCandidate, RunnerName};
use yunta_render::command;

use super::{ProjectSetup, ProposedRunners, RunnerDeclaration};

/// The project's config as `init` writes it: its name and base branch, what
/// the repository was detected to run, and its runners, declared as `setup`
/// says or proposed, with each adapter this machine answered for, healthy or
/// not.
pub(super) fn config_yaml(
    setup: &ProjectSetup,
    probed: &[ProbedAdapter],
    detected: &Detected,
    proposed: &ProposedRunners,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Written by `{}`: team-shared, commit this file.\n",
        command::line(&NextStep::Init)
    ));
    out.push_str("# Personal overrides belong in ~/.yunta/config.yaml (the user\n");
    out.push_str("# config layer), never in this file.\n\n");
    out.push_str("project:\n");
    out.push_str(&format!("  name: {}\n", yaml_scalar(&setup.name)));
    out.push_str(&format!(
        "  base_branch: {}\n",
        yaml_scalar(&setup.base_branch)
    ));
    out.push_str("  branch_prefix: yunta/\n\n");
    out.push_str(&detected_yaml(detected));

    out.push_str("# `runners:` names roles your workflows' `runner:` fields reference,\n");
    out.push_str("# each with one or more adapter candidates (first capable one wins).\n");
    for adapter in probed {
        if adapter.healthy {
            out.push_str(&format!(
                "# detected: {} ({})\n",
                adapter.id, adapter.detail
            ));
        } else {
            out.push_str(&format!(
                "# not available: {} ({})\n",
                adapter.id, adapter.detail
            ));
        }
    }
    let declared = match &setup.runners {
        RunnerDeclaration::Declared(declared) if !declared.is_empty() => declared,
        _ => {
            out.push_str(&proposed_runners_yaml(probed, proposed));
            return out;
        }
    };
    out.push_str("runners:\n");
    for role in &proposed.roles {
        match declared.get(role) {
            Some(candidate) => out.push_str(&declared_runner_yaml(role, candidate)),
            None => out.push_str(&proposed_runner_yaml("  # ", role, probed)),
        }
    }
    let default = proposed.roles.first().filter(|_| proposed.needs_default);
    match default {
        Some(role) if declared.contains_key(role) => {
            out.push_str(&format!("defaults:\n  runner: {role}\n"));
        }
        Some(role) => out.push_str(&format!("# defaults:\n#   runner: {role}\n")),
        None => {}
    }
    out
}

/// The runners `proposed` names, commented out, each on the first adapter that
/// answered and its model left blank; and `defaults.runner`, commented, where a
/// node names none.
fn proposed_runners_yaml(probed: &[ProbedAdapter], proposed: &ProposedRunners) -> String {
    let mut out = String::from("# runners:\n");
    for role in &proposed.roles {
        out.push_str(&proposed_runner_yaml("#   ", role, probed));
    }
    if let (true, Some(first)) = (proposed.needs_default, proposed.roles.first()) {
        out.push_str(&format!("# defaults:\n#   runner: {first}\n"));
    }
    out
}

/// One proposed runner, commented out after `lead`: on the first adapter that
/// answered, or the first this build knows when none did, its model blank.
fn proposed_runner_yaml(lead: &str, runner: &RunnerName, probed: &[ProbedAdapter]) -> String {
    let adapter = match probed.iter().find(|adapter| adapter.healthy) {
        Some(healthy) => healthy.id.to_string(),
        None => crate::machine::first_built_adapter().to_string(),
    };
    format!("{lead}{runner}:\n{lead}  - {{ adapter: {adapter}, model: <model-name> }}\n")
}

/// One declared runner, as a person answered it: on its adapter, with its model,
/// which YAML reads back whole whatever it holds.
fn declared_runner_yaml(runner: &RunnerName, candidate: &RunnerCandidate) -> String {
    format!(
        "  {runner}:\n    - adapter: {}\n      model: {}\n",
        candidate.adapter,
        yaml_scalar(candidate.model.as_str())
    )
}

/// What `init` found in the repository, as config: the commands a
/// workflow asks for by name, the suite a run measures when it reads the
/// measurement, and the forge `origin` is on.
fn detected_yaml(detected: &Detected) -> String {
    let mut out = String::new();
    if !detected.commands.is_empty() {
        out.push_str("# What this project runs for each capability a workflow names\n");
        out.push_str(&format!(
            "# (`run: {{ command: lint }}`), as `{}` detected it.\n",
            command::line(&NextStep::Init)
        ));
        out.push_str("commands:\n");
        for (name, text) in &detected.commands {
            out.push_str(&format!("  {name}: {}\n", yaml_string(text)));
        }
        out.push('\n');
    }
    if let Some(suite) = &detected.suite {
        out.push_str("# Measured before a run's first node when the run reads it.\n");
        out.push_str(&format!("baseline:\n  suite: {}\n\n", yaml_string(suite)));
    }
    if let Some(repo) = &detected.forge {
        out.push_str("# Where a `pull_request` node opens its pull request.\n");
        out.push_str(&format!(
            "forge:\n  github:\n    repo: {repo}\n    token_env: GITHUB_TOKEN\n\n"
        ));
    }
    out
}

/// `text` as YAML writes a string: as it is when nothing in it reads as
/// anything else, quoted when a comment, a mapping or another type would cut
/// or change it.
fn yaml_scalar(text: &str) -> String {
    yunta_core::yaml::to_string(&text)
        .map(|written| written.trim_end().to_string())
        .unwrap_or_else(|_| yaml_string(text))
}

/// `text` as a double-quoted YAML scalar.
fn yaml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests;
