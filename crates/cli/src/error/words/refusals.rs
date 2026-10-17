//! The sentences for what `check` refuses about a workflow.

use yunta_core::events::DeniedRule;
use yunta_core::{ArtifactKind, ArtifactSpec, ConfigKey, NodeId, ReservedIdentity};
use yunta_engine::CheckError;

use super::wrapped::{composition_words, unanswerable_words, untaken_words};

/// What `check` refuses, in the words a person reads it as.
///
/// Each family names the variants it writes and answers `None` for the rest,
/// so every variant is written by exactly one family. A variant no family
/// names is written as its code, which shows the gap instead of an empty
/// sentence.
pub(crate) fn check(error: &CheckError) -> String {
    node_words(error)
        .or_else(|| session_words(error))
        .or_else(|| runner_words(error))
        .or_else(|| config_words(error))
        .or_else(|| artifact_words(error))
        .or_else(|| answer_words(error))
        .or_else(|| group_words(error))
        .or_else(|| input_words(error))
        .or_else(|| pack_words(error))
        .or_else(|| permission_words(error))
        .or_else(|| wrapped_words(error))
        .unwrap_or_else(|| error.to_string())
}

/// What a node declares about its own shape.
fn node_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::DependsOnCycle { cycle } => {
            format!("cycle in depends_on: {}", cycle_words(cycle))
        }
        CheckError::BothRunnerAndRunners { node } => {
            format!("node `{node}` declares both `runner:` and `runners:` — use exactly one")
        }
        CheckError::FanOutTarget { node, target } => format!(
            "node `{node}` targets `{target}`, which is a `runners:` fan-out — target one of its \
             expanded nodes (`{target}@<role>`) or a non-fan-out node"
        ),
        CheckError::WorkflowNodeRunnerBinding { node, field } => format!(
            "node `{node}`: `{field}` has no meaning on `kind: workflow` — the child workflow's \
             own nodes bind their runners"
        ),
        CheckError::ContextOnUnsupportedNode { node } => format!(
            "node `{node}`: `context:` is only supported on `kind: prompt` and `kind: loop` nodes \
             — nothing else opens a session that could consume it"
        ),
        _ => return None,
    })
}

/// What a node's session and gate can do, and what they cannot.
fn session_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::ResumeSessionOnSessionlessNode { node } => format!(
            "node `{node}`: `on_interrupt: resume_session` is only supported on `kind: prompt` \
             nodes — nothing else has a node-scoped session to resume; declare `restart_node` \
             (the default) instead"
        ),
        CheckError::AskOnANodeThatAsksNothing { node } => format!(
            "node `{node}`: `ask` says which rounds of a node's questions stop the run, and this \
             node produces no `questions` — remove `ask`, or set it on the node that asks"
        ),
        CheckError::ShowsOnExternalGate { node } => format!(
            "gate `{node}`: `shows:` is for a gate answered here; an external gate publishes its \
             `artifacts:`"
        ),
        CheckError::ExternalGateWithoutForge { node } => format!(
            "node `{node}`: `kind: gate` with `external: {{kind: pull_request}}` needs `forge.github` configured"
        ),
        _ => return None,
    })
}

/// What a runner the session binds to cannot be, and what it cannot run.
fn runner_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::CapabilityUnsupported {
            node,
            field,
            capability,
            adapters,
        } => format!(
            "node `{node}` declares `{field}`, which needs `{capability}`, and no adapter its \
             runner resolves to declares it ({adapters}) — pick a runner on an adapter that has it"
        ),
        CheckError::UnknownRunner { node, runner, near } => format!(
            "node `{node}` references runner `{runner}`, which `runners:` does not define{}",
            did_you_mean(near.as_ref())
        ),
        CheckError::RunnerHasNoCandidates { node, runner } => format!(
            "node `{node}` references runner `{runner}`, which `runners:` defines with zero candidates"
        ),
        CheckError::CommandDenied { node, denied } => format!(
            "node `{node}`: {}",
            yunta_render::failure::denied(&DeniedRule::Command(denied.clone()))
        ),
        _ => return None,
    })
}

/// What the config must declare for a node to run, and what it cannot be.
fn config_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::Unset { node, key, near } => {
            format!("node `{node}`: {key}{}", did_you_mean(near.as_ref()))
        }
        CheckError::CommandUnsetIn {
            field,
            command,
            near,
        } => format!(
            "`{field}` names `{command}`, which `commands:` does not declare — declare it, or drop \
             it{}",
            did_you_mean(near.as_ref())
        ),
        CheckError::MaxParallelNodesZero => {
            "`defaults.max_parallel_nodes: 0` would starve every node forever — declare 1 or more, \
             or drop the field (default: 1)"
                .to_string()
        }
        CheckError::BaselineWithoutSuite { sites } => {
            format!("{sites}: {}", ConfigKey::BaselineSuite)
        }
        _ => return None,
    })
}

/// What a document or an artifact name claims, and where a reader meets it.
fn artifact_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::ContextFileUnreachable {
            node,
            path,
            base,
            missing,
        } => yunta_render::caution::missing_sentence(
            node,
            path,
            base.as_deref(),
            missing,
            yunta_render::caution::Reach::NothingEarlierCan,
        ),
        CheckError::DistillUnknownArtifact { artifact } => format!(
            "`on_finish.distill` names {artifact} but that node's `artifacts.produces` does not \
             declare it — declare the artifact on the node that produces it, or drop it from \
             `distill`"
        ),
        CheckError::DuplicateArtifactKind { node, kind } => format!(
            "node `{node}` declares `{kind}` twice — a node produces at most one {}, and it is \
             identified by its kind, so there is no second one to declare",
            kind.label()
        ),
        CheckError::InputDocumentAlsoProduced { input, node, kind } => format!(
            "input `{input}` brings in the {} and node `{node}` produces one too — a run resolving \
             `{kind}` would get whichever landed last, which nothing in the workflow decides; drop \
             the input, or `{kind}` from node `{node}`'s `artifacts.produces`",
            kind.label()
        ),
        CheckError::ReservedArtifactName { place, name } => format!(
            "{} names the artifact `{name}`, and `{name}` is a document Yunta reads — refer to it \
             with `kind: {name}` instead; the names {} are not available as file names",
            yunta_render::place::label(place),
            ArtifactKind::listed()
        ),
        CheckError::ArtifactNameRefused {
            node,
            name,
            problem,
        } => format!(
            "node `{node}` produces a name that {}",
            yunta_render::failure::name(name, problem)
        ),
        _ => return None,
    })
}

/// What a node asks, answers or gates, and what it cannot do with them.
fn answer_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::QuestionsAlongsideOtherArtifacts { node, others } => format!(
            "node `{node}` produces `questions` alongside {} — a node that asks ends when it asks, \
             and its answers reach the next node as context; keep `{node}` producing `questions` \
             alone and move {} to a node that follows it with `context: [{{ artifact: {{ node: \
             {node}, {} }} }}]`",
            ArtifactSpec::listed(others),
            ArtifactSpec::listed(others),
            ReservedIdentity::Kind(ArtifactKind::Answers).reference()
        ),
        CheckError::AnswersDeclaredAsProduced { node } => format!(
            "node `{node}` produces `answers` — the engine writes the answers to a `questions` \
             document itself when a person replies; a node produces one of {} or a file name, and \
             the node that follows the one that asked reads the answers with `context: \
             [{{ artifact: {{ node: {node}, kind: answers }} }}]`",
            ArtifactKind::declarable_listed()
        ),
        CheckError::AnswersFromNodeThatNeverAsks { node, place } => format!(
            "{} reads `kind: answers` from node `{node}`, and `{node}` asks nothing — only a node \
             that produces `questions` leaves answers; read them from the node that asks, or drop \
             the source",
            yunta_render::place::label(place)
        ),
        CheckError::QuestionsOnKind { node, kind } => format!(
            "node `{node}` is `kind: {kind}` and produces `questions` — only a `prompt` node asks; \
             put the questions in a `prompt` node and read its answers from here"
        ),
        CheckError::QuestionsInsideParallel { node, group } => format!(
            "node `{node}` produces `questions` inside parallel group `{group}` — a person answers \
             one node at a time; ask before or after the group"
        ),
        CheckError::GateRouteNeverTaken { node, route } => {
            format!("gate `{node}`: {}", untaken_words(route))
        }
        _ => return None,
    })
}

/// What a group of parallel children, a mount or a shared scope cannot be.
fn group_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::OverlappingFanOutScope {
            a,
            b,
            glob_a,
            glob_b,
        } => format!(
            "nodes `{a}` and `{b}` have no dependency path between them and declare overlapping \
             scope (`{glob_a}` / `{glob_b}`) — with `max_parallel_nodes` > 1 they can write the same \
             paths at once; chain them with `depends_on` or make their scopes disjoint"
        ),
        CheckError::GateInsideParallel { node, group } => {
            format!("node `{node}`: `kind: gate` can't be a `parallel` child (group `{group}`)")
        }
        CheckError::ParallelInsideParallel { node, group } => {
            format!("node `{node}`: `kind: parallel` can't be a `parallel` child (group `{group}`)")
        }
        CheckError::InheritChildWithoutScope { group, node } => format!(
            "parallel group `{group}`: child `{node}` is `kind: workflow` with `isolation: none` and \
             no `scope` — a child sharing the tree writes where its siblings do, so each must \
             declare a disjoint scope"
        ),
        CheckError::MountOnSelf { node } => format!(
            "node `{node}`: `mounts` references the node itself — a mount reads a *finished* node's \
             artifact, which this node cannot be for its own birth"
        ),
        CheckError::MountOnFanOut { node, target } => format!(
            "node `{node}`: `mounts` references `{target}`, which `runners:` fans out into one node \
             per role — mount a specific `{target}@<role>` sibling instead"
        ),
        CheckError::MountInsideParallel { group, node } => format!(
            "parallel group `{group}`: child `{node}` declares `mounts` — parallel children have no \
             order to guarantee a finished source; mount on a top-level workflow node instead"
        ),
        _ => return None,
    })
}

/// What an input declaration or the schema range cannot be.
fn input_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::YuntaSchemaOutside { range, binary } => format!(
            "`yunta_schema: \"{range}\"` — this binary's schema is outside the required range (this \
             binary speaks schema {binary})"
        ),
        CheckError::InputEmptyEnumValues { name } => {
            format!("input `{name}` is type `enum` with an empty `values` list")
        }
        CheckError::InputMinExceedsMax { name, min, max } => {
            format!("input `{name}`'s `min` ({min}) is greater than its `max` ({max})")
        }
        CheckError::InputInvalidPattern {
            name,
            pattern,
            detail,
        } => format!("input `{name}`'s `pattern` `{pattern}` is not a valid regex: {detail}"),
        CheckError::UndeclaredInput { node, name } => format!(
            "node `{node}` references `{{{{inputs.{name}}}}}`, which `inputs:` does not declare"
        ),
        _ => return None,
    })
}

/// What a pack's manifest says it needs, and why it cannot be read.
fn pack_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::PackManifestUnreadable { path, detail } => {
            format!(
                "pack manifest `{}` cannot be read: {detail}",
                path.display()
            )
        }
        CheckError::PackManifestMalformed { path, detail } => {
            format!("pack manifest `{}` is malformed: {detail}", path.display())
        }
        CheckError::PackRequirementUnmet { pack, requirement } => {
            format!("pack `{pack}` requires {requirement}")
        }
        _ => return None,
    })
}

/// What a node may ask for in network and scope, against what it runs under.
fn permission_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::NetworkOpenUnderReadOnly { node } => format!(
            "node `{node}` asks for `network: true`, but its session runs read-only and gets no \
             network: drop `network: true`, or let it write with `permissions: edit` under a \
             non-read-only `sandbox`"
        ),
        CheckError::ClosedNetworkUnconfined { node } => format!(
            "node `{node}`: the network is closed, but its session runs unconfined, so nothing keeps \
             its commands off it — lower `permissions` to `edit` under a confining `sandbox`, or set \
             `network: true`"
        ),
        CheckError::ScopeExpansionModeOverCeiling {
            node,
            mode,
            ceiling,
        } => format!(
            "node `{node}`: `scope_expansion.mode: {mode}` exceeds the merged permissions ceiling \
             `scope_expansion.max_mode: {ceiling}` — harden the node's mode, or raise the ceiling in \
             the layer that set it"
        ),
        CheckError::ScopeExpansionWithinRetired { node } => format!(
            "node `{node}`: `scope_expansion.within` moved to `permissions.paths.deny`, the project's \
             list of paths no run may write — remove `within` from this node"
        ),
        CheckError::PackPermissionsCeilingExceeded {
            node,
            pack,
            declared,
            effective,
        } => format!(
            "node `{node}` requests permissions `{effective}` but pack `{pack}` declares a ceiling of \
             `{declared}` — lower the node's permissions or raise the pack's declared ceiling"
        ),
        CheckError::PackNetworkCeilingExceeded { node, pack } => format!(
            "node `{node}` asks for `network: true`, but pack `{pack}` declares the network closed, \
             so drop `network: true`"
        ),
        _ => return None,
    })
}

/// The two refusals that wrap a sub-enum, each written by its own module.
fn wrapped_words(error: &CheckError) -> Option<String> {
    Some(match error {
        CheckError::Unanswerable(unanswerable) => unanswerable_words(unanswerable),
        CheckError::Composition(composition) => composition_words(composition),
        _ => return None,
    })
}

/// `a -> b -> a`: a cycle as a sentence names it.
fn cycle_words(cycle: &[NodeId]) -> String {
    cycle
        .iter()
        .map(NodeId::as_str)
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// ` — did you mean `name`?` when a refusal holds a near miss.
fn did_you_mean(near: Option<&String>) -> String {
    near.map(|near| format!(" — did you mean `{near}`?"))
        .unwrap_or_default()
}
