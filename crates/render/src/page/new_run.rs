//! A new run's main region: what a person sets before a run starts, and what the run does
//! with it. The request is asked as the first request asks it, in the same box with the same
//! placeholder ([`super::FirstRequest`]), and under it stands the hint: what the workflow
//! declares of the input the request stands in, then, where a node the chosen mode runs asks
//! questions, how the workflow asks: before it decides for the person where every round waits
//! for its answers, only what it cannot decide for them where a round waits only on a required
//! question. No clause says what the chosen workflow does not do. Each other input is labelled by its name and its kind over
//! its box, what the workflow declares of it under the box.
//!
//! A workflow that reads no request says so in the request's place under the box, so words
//! typed there are never taken for one. The headings of the rows a person sets (`workflow`,
//! `mode`, `adapter`) are worded here, strong on the row the keys act on and muted on the others,
//! with the adapter row's first word. Under the rows stands `what runs` ([`NewRun::what_runs`]):
//! a stroke for each node the workflow declares, the nodes the chosen mode leaves out drawn as
//! left out; the names of those it runs, each as the ribbon over a run names its node
//! (`watched::stretch_names` over every node's id); and each gate it runs, where the run stops
//! for the person. It moves with the workflow and the mode chosen and with nothing else. The
//! rows themselves are the stage's that holds them, each in these words; the keys, by the row
//! in focus, are the screen's
//! (`screen::new_run_legend`). Beside the region stands the panel ([`WhatItTakes`], in
//! `takes`): what the workflow usually takes, and what this machine answers for.

use surco::components::{FormFoot, Stretch, TypedAnswer};
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::words::Standing;
use yunta_core::{Ask, InputName, InputSpec, ModeName, Node, NodeId, NodeKind, Workflow};

use super::FirstRequest;
use crate::prompt::typed_answer;

/// A new run of a workflow chosen, in a mode chosen: the words its main region says of them.
/// Not `RunRequest`, what the rows build.
pub struct NewRun<'a> {
    /// The workflow chosen, as it reads.
    pub workflow: &'a Workflow,
    /// The mode chosen: the workflow's first until another is, as `yunta run` starts it when
    /// none is named.
    pub mode: &'a ModeName,
}

impl NewRun<'_> {
    /// The heading over the workflows a new run can be of.
    pub const WORKFLOW: &'static str = "workflow";

    /// The heading over the modes the chosen workflow declares.
    pub const MODE: &'static str = "mode";

    /// The heading over the adapters a new run's sessions can be pinned to.
    pub const ADAPTER: &'static str = "adapter";

    /// The adapter row's first word: no adapter pinned, the one each runner names taken.
    pub const AS_THE_RUNNERS_SAY: &'static str = "as the runners in config.yaml say";

    /// The heading over what the chosen workflow runs in the chosen mode.
    pub const WHAT_RUNS: &'static str = "what runs";

    /// The request, asked as the first request asks it, with the words typed `before` and
    /// `after` the cursor in its box, and the hint under it.
    pub fn request(&self, before: &str, after: &str) -> TypedAnswer {
        TypedAnswer {
            hint: self.hint(),
            ..FirstRequest::boxed(before, after)
        }
    }

    /// What stands under the request: what the workflow declares of the input the request
    /// stands in, or that it takes no request where no input does, then how the workflow asks
    /// where a node the chosen mode runs asks questions; none where neither stands.
    pub fn hint(&self) -> Option<String> {
        let request = self.workflow.inputs.values().find(|spec| spec.is_request());
        let declared = match request {
            Some(spec) => spec.description().map(str::to_string),
            None => Some(format!("{} takes no request", self.workflow.name)),
        };
        let clauses: Vec<String> = declared.into_iter().chain(self.asking()).collect();
        (!clauses.is_empty()).then(|| clauses.join("; "))
    }

    /// How the workflow asks the person in the chosen mode, as [`Workflow::ask`] says it for
    /// each node the mode runs that asks: before it decides for them where every round waits
    /// for its answers; only what it cannot decide for them where every round waits only on a
    /// required question, the rest going on with what each assumes; where the two mix, before
    /// it decides at each node whose round always waits, named, and elsewhere only what it
    /// cannot decide. None where no node the mode runs asks.
    fn asking(&self) -> Option<String> {
        let asks: Vec<(&NodeId, Ask)> = self
            .runs()
            .into_iter()
            .filter(|node| node.asks())
            .map(|node| (&node.id, self.workflow.ask(node, self.mode)))
            .collect();
        let name = &self.workflow.name;
        if asks.is_empty() {
            return None;
        }
        let always: Vec<String> = asks
            .iter()
            .filter(|(_, ask)| *ask == Ask::Always)
            .map(|(node, _)| node.to_string())
            .collect();
        Some(match (always.is_empty(), always.len() == asks.len()) {
            (false, true) => format!("{name} asks before it decides for you"),
            (true, _) => format!("{name} asks only what it cannot decide for you"),
            (false, false) => format!(
                "{name} asks before it decides for you at {}, and elsewhere only what it \
                 cannot decide",
                crate::screen::named(&always)
            ),
        })
    }

    /// Where the chosen mode stops for the person, after the mark that means them: each gate
    /// it runs, in the order the workflow declares them; none where it runs no gate.
    pub fn stops(&self, glyphs: Glyphs) -> Option<Line> {
        let gates: Vec<String> = self
            .runs()
            .into_iter()
            .filter(|node| matches!(node.kind, NodeKind::Gate { .. }))
            .map(|node| node.id.to_string())
            .collect();
        (!gates.is_empty()).then(|| {
            Line::new()
                .push(Tone::NeedsYou, glyphs.mark(Mark::NeedsYou).to_string())
                .plain(format!(
                    " it stops for you at {}",
                    crate::screen::named(&gates)
                ))
        })
    }

    /// The heading of a row a person sets, `word`: strong on the row the keys act on, `focused`,
    /// and muted on the others, so a layer with tones says where the keys are.
    pub fn heading(word: &str, focused: bool) -> Line {
        let tone = match focused {
            true => Tone::Strong,
            false => Tone::Muted,
        };
        Line::new().push(tone, word)
    }

    /// An input other than the request, as its row is labelled over its box: its name, then
    /// what kind of value it takes and whether it is required; its name strong on the row the
    /// keys act on, `focused`, and the whole label muted on any other.
    pub fn input_label(name: &InputName, spec: &InputSpec, focused: bool) -> Line {
        let kind = format!(" ({})", crate::inputs::declared(spec));
        match focused {
            true => Line::new().push(Tone::Strong, name.to_string()).plain(kind),
            false => Line::new().push(Tone::Muted, format!("{name}{kind}")),
        }
    }

    /// An input other than the request, asked: its label over the box with the words typed
    /// `before` and `after` the cursor, and what the workflow declares of it under the box; its
    /// label as the row the keys act on says it where it is `focused`.
    pub fn input(
        name: &InputName,
        spec: &InputSpec,
        (before, after): (&str, &str),
        focused: bool,
    ) -> TypedAnswer {
        let label = Self::input_label(name, spec, focused);
        TypedAnswer {
            hint: spec.description().map(str::to_string),
            ..typed_answer(&label, before.into(), after.into())
        }
    }

    /// What runs in the chosen mode, under the rows a person sets, in `glyphs`: a stroke for
    /// each node the workflow declares, in its order, by its name as the ribbon names it among
    /// every node's, left out where the mode leaves it out; the names of the nodes it runs; then
    /// where it stops for the person, where it stops.
    pub fn what_runs(&self, glyphs: Glyphs) -> FormFoot {
        let runs: Vec<&NodeId> = self.runs().into_iter().map(|node| &node.id).collect();
        let nodes: Vec<&Node> = self.workflow.iter_nodes().collect();
        let ids: Vec<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
        let strokes: Vec<Stretch> = nodes
            .iter()
            .zip(crate::watched::stretch_names(&ids))
            .map(|(node, name)| match runs.contains(&&node.id) {
                true => Stretch::new(name, Standing::NeverRan),
                false => Stretch::new(name, Standing::Skipped),
            })
            .collect();
        let named: Vec<&str> = strokes
            .iter()
            .filter(|step| step.standing == Standing::NeverRan)
            .map(|step| step.name.as_str())
            .collect();
        let named = Line::new().plain(named.join(" "));
        FormFoot {
            heading: Line::new().push(Tone::Muted, Self::WHAT_RUNS),
            strokes,
            said: std::iter::once(named).chain(self.stops(glyphs)).collect(),
        }
    }

    /// Every node a run of the chosen workflow reaches in the chosen mode, in the order the
    /// workflow declares them.
    fn runs(&self) -> Vec<&Node> {
        yunta_replay::modes::reached(self.workflow, self.mode, &[])
    }
}

mod takes;
#[cfg(test)]
mod tests;

pub use takes::WhatItTakes;
