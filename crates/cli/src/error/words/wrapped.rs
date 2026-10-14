//! The sentences for the refusals that wrap a sub-enum: a read nothing in the
//! run can answer, a `use:` reference that refuses, and a gate route no
//! answer follows.

use yunta_core::text::counted;
use yunta_core::ModeName;
use yunta_engine::{Composition, Unanswerable, UntakenRoute};

use super::refusals::check;

/// A read nothing in the run can answer, in the words of the reader it names.
pub(super) fn unanswerable_words(error: &Unanswerable) -> String {
    match error {
        Unanswerable::LoopWithoutTasks { node, mode } => format!(
            "loop `{node}` works through the run's tasks document, and nothing{} gives it one — \
             declare `produces: [tasks]` on a node that runs before it, declare a `type: document` \
             input of `kind: tasks`, or mount one from the workflow that composes this one",
            in_mode(mode)
        ),
        Unanswerable::ArtifactNotDeclared {
            place,
            node,
            artifact,
        } => format!(
            "{} reads the {} of node `{node}`, which does not declare it in `artifacts.produces` — \
             declare it on `{node}`, or read it from the node that produces it",
            yunta_render::place::label(place),
            artifact.label()
        ),
        Unanswerable::ArtifactFromNowhere {
            place,
            artifact,
            mode,
        } => format!(
            "{} reads the run's {}, and nothing{} can hold one: no node declares it, no input brings \
             it and no workflow composing this one mounts it — declare it on the node that writes it",
            yunta_render::place::label(place),
            artifact.label(),
            in_mode(mode)
        ),
        Unanswerable::SourceLeftOut { place, from, mode } => format!(
            "{} reads from `{from}`, which mode `{mode}` leaves out — a run started in `{mode}` holds \
             nothing of it; start an earlier mode and promote, or include `{from}`",
            yunta_render::place::label(place)
        ),
        Unanswerable::NodeOutputOfNonBash {
            node,
            referenced,
            kind,
        } => format!(
            "node `{node}` reads the output of `{referenced}`, {} — only a `kind: bash` node \
             captures output",
            match kind {
                Some(kind) => format!("a `kind: {kind}` node"),
                None => "which the workflow does not declare".to_string(),
            }
        ),
    }
}

/// The `on:` route no answer follows, in the words a gate's reader sees.
pub(super) fn untaken_words(route: &UntakenRoute) -> String {
    match route {
        UntakenRoute::Undeclared(option) => format!(
            "`on.{option}` maps an option `options:` does not declare, so no one can choose it"
        ),
        UntakenRoute::Abort => {
            "`on.abort` is never taken — choosing `abort` pauses the run; give the option that sends \
             the run back another id"
                .to_string()
        }
    }
}

/// What a `use:` reference refuses, with each problem of the child said in
/// the same words, joined in order.
pub(super) fn composition_words(error: &Composition) -> String {
    match error {
        Composition::Fails {
            node,
            name,
            problems,
        } => format!(
            "node `{node}`: `use: {name}` fails check — {}",
            problems.iter().map(check).collect::<Vec<_>>().join("; ")
        ),
        Composition::Missing { node, name, detail } => {
            format!("node `{node}`: `use: {name}` cannot be resolved — {detail}")
        }
        Composition::Ambiguous { node, name, detail } => {
            format!("node `{node}`: `use: {name}` is ambiguous — {detail}")
        }
        Composition::CrossPack {
            node,
            name,
            from_pack,
        } => format!(
            "node `{node}`: `use: {name}` reaches outside pack `{from_pack}` — composition across \
             packs isn't supported; copy what you need into your own pack instead"
        ),
        Composition::Unparseable { path, detail } => format!(
            "workflow `{}` (referenced through composition) does not parse: {detail}",
            path.display()
        ),
        Composition::Cycle { chain } => format!("workflow composition cycle: {chain}"),
        Composition::TooDeep { chain, depth, max } => format!(
            "workflow composition {chain} nests {} deep but `limits.max_workflow_depth` is {max} — \
             flatten the composition or raise the limit",
            counted(*depth as usize, "level")
        ),
    }
}

/// How a refusal that holds in one mode names it: nothing when the workflow
/// declares no modes, since then there is one graph to be.
fn in_mode(mode: &Option<ModeName>) -> String {
    mode.as_ref()
        .map(|mode| format!(" in mode `{mode}`"))
        .unwrap_or_default()
}
