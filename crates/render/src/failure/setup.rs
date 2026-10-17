//! The words of what a node needs ahead of its run: the runner it names, the
//! capability its adapter must declare, the skill it mounts, and the frozen
//! inputs and reads the run was born with.

use yunta_core::events::{CapabilityUse, Failure, FrozenRead, RunnerRefusal};
use yunta_core::text;

/// A runner that did not resolve: no entry defines it, or none of its
/// candidates is available.
pub(super) fn runner_unresolved(failure: &Failure) -> String {
    let Failure::RunnerUnresolved {
        runner,
        runner_refusal,
    } = failure
    else {
        unreachable!("routed here only for an unresolved runner")
    };
    match runner_refusal {
        RunnerRefusal::Unknown => {
            format!("no `runners:` entry defines runner `{runner}` — add it to the merged config")
        }
        RunnerRefusal::NoCandidate { tried } => format!(
            "runner `{runner}` has no available candidate — tried {}: {}",
            text::agreeing(tried.len(), "adapter", "adapters"),
            tried
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// A capability the node needs that its adapter does not declare, in the
/// words of what the node needs it for.
pub(super) fn capability_missing(failure: &Failure) -> String {
    let Failure::CapabilityMissing {
        node,
        capability,
        adapter,
        used_for,
        agent,
    } = failure
    else {
        unreachable!("routed here only for a missing capability")
    };
    match (used_for, agent) {
        (Some(CapabilityUse::Blackboard), _) => format!(
            "node `{node}` is in a `coordination: blackboard` group but adapter `{adapter}` declares no \
             `run_tools` capability — the blackboard cannot be mounted; pick a runner on an \
             adapter that can be a client of the per-run MCP endpoint"
        ),
        (Some(CapabilityUse::TypedArtifact { kind }), _) => format!(
            "node `{node}` declares a `{kind}` artifact, which a session hands over through the run \
             tools, and adapter `{adapter}` declares no `run_tools` capability — the document \
             has no way in; pick a runner on an adapter that can be a client of the per-run MCP \
             endpoint"
        ),
        (Some(CapabilityUse::LoopTasks), _) => format!(
            "node `{node}` is a loop, whose task sessions read their task and check their work through \
             the run tools, and adapter `{adapter}` declares no `run_tools` capability — a \
             session there could not learn what its task asks; pick a runner on an adapter that \
             can be a client of the per-run MCP endpoint"
        ),
        (None, Some(agent)) => format!(
            "a node requests agent `{agent}` but adapter `{adapter}` does not declare \
             `custom_agents` — pick a candidate on an adapter that does, or drop the agent"
        ),
        (None, None) => format!(
            "adapter `{adapter}` does not declare `{capability}`, which a node of this run \
             requests — pick a candidate on an adapter that does, or drop the request"
        ),
    }
}

/// A skill the node names that the run does not hold: the roots it searched,
/// when the failure says them, and what to do about it.
pub(super) fn skill_missing(failure: &Failure) -> String {
    let Failure::SkillMissing {
        missing_skill,
        searched,
    } = failure
    else {
        unreachable!("routed here only for a missing skill")
    };
    let advice = "add the directory, fix `skills.paths` in the config, or install the pack that \
                  declares it";
    match searched.is_empty() {
        true => format!("skill `{missing_skill}` not found — {advice}"),
        false => format!(
            "skill `{missing_skill}` not found under {} — {advice}",
            searched
                .iter()
                .map(|root| format!("`{}`", root.display()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// A frozen input whose bytes do not match the hash the run was born with.
pub(super) fn frozen_input(input: &str) -> String {
    format!("the frozen input `{input}` no longer matches what the run was born with")
}

/// A frozen read that cannot give the node what it asked for.
pub(super) fn frozen_read(read: &FrozenRead) -> String {
    match read {
        FrozenRead::Unresolved { problem } => problem.clone(),
        FrozenRead::Escapes { pack, path } => {
            format!("pack `{pack}` declares `{path}`, which leaves the pack's own directory")
        }
        FrozenRead::Prompt { node, path } => format!(
            "node `{node}` references prompt file `{}` which cannot be read",
            path.display()
        ),
    }
}
