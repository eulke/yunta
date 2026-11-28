//! See [`super`]. The project's config as `init` writes it, YAML a person reads and
//! commits: its name and base branch, what the repository was detected to run, and its
//! runners, each declared on its candidates in order or proposed, commented, where no
//! adapter answered, beside the adapters this machine answered for, and the default
//! runner the proposal names. Every value a person typed is written so it reads back
//! whole (`tests`).

use yunta_core::step::NextStep;
use yunta_core::{Detected, ProbedAdapter, RunnerName};
use yunta_render::command;

use super::{DeclaredRunner, ProjectSetup, RunnerDeclaration};

/// The project's config as `init` writes it: its name and base branch, what
/// the repository was detected to run, and its runners, declared as `setup`
/// says or proposed, with each adapter this machine answered for, healthy or
/// not, and the default runner the proposal `setup` was answered from names.
pub(super) fn config_yaml(
    setup: &ProjectSetup,
    probed: &[ProbedAdapter],
    detected: &Detected,
) -> String {
    let proposed = setup.proposed();
    let mut out = String::new();
    out.push_str(&format!(
        "# Written by `{}`: team-shared, commit this file.\n",
        command::line(&NextStep::Init { interactive: false })
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
    let default = proposed.default_runner();
    let declared = match setup.runners() {
        RunnerDeclaration::Declared(declared) => declared,
        RunnerDeclaration::Proposed => {
            out.push_str(&proposed_runners_yaml(proposed.roles(), default));
            return out;
        }
    };
    out.push_str("runners:\n");
    for runner in declared {
        out.push_str(&declared_runner_yaml(runner));
    }
    if let Some(default) = default {
        out.push_str(&format!("defaults:\n  runner: {default}\n"));
    }
    out
}

/// The runners `roles` names, commented out, each on the first adapter this build
/// knows, as none answered on this machine, its model left to it; and
/// `defaults.runner`, commented, where the proposal names a `default`.
fn proposed_runners_yaml(roles: &[RunnerName], default: Option<&RunnerName>) -> String {
    let adapter = crate::machine::first_built_adapter();
    let mut out = String::from("# runners:\n");
    for runner in roles {
        out.push_str(&format!(
            "#   {runner}:\n#     - {{ adapter: {adapter} }}\n"
        ));
    }
    if let Some(default) = default {
        out.push_str(&format!("# defaults:\n#   runner: {default}\n"));
    }
    out
}

/// One runner `declared` on its candidates, in the order a run tries them: each on its
/// adapter, with its model where one was typed, which YAML reads back whole whatever it
/// holds.
fn declared_runner_yaml(declared: &DeclaredRunner) -> String {
    let mut out = format!("  {}:\n", declared.runner);
    for candidate in &declared.candidates {
        out.push_str(&format!("    - adapter: {}\n", candidate.adapter));
        if let Some(model) = &candidate.model {
            out.push_str(&format!("      model: {}\n", yaml_scalar(model.as_str())));
        }
    }
    out
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
            command::line(&NextStep::Init { interactive: false })
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
