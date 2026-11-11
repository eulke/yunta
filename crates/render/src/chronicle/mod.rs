//! The words for one moment of a run's chronicle, chosen once.
//!
//! `yunta_replay::frame::chronicle` folds a log into its moments; this says
//! each. Every surface that says what happened says it from here: the
//! append-only lines a pipe gets, the history a watched terminal keeps above
//! its region, and the run as the user interface follows it. They differ in
//! what they *keep* and in how they lay it out, never in what a thing is
//! called, so a reader who followed a run on a terminal and a reader who
//! read the same run out of a CI log met the same sentences. What a surface
//! keeps of a moment is that surface's.
//!
//! Every state word comes from [`crate::state::NodeDisplay`], every run or
//! child close from [`crate::children::closed_as`], every capability from
//! `Capability::as_str`. Nothing here formats a domain type with `Debug`: a
//! reader is owed a word, not a Rust identifier.

mod baseline_words;
mod capability_words;
mod format_words;
mod rail_words;
mod scope_words;
mod task_words;
mod words;

use surco::components::Whole;
use surco::doc::{Block, Depth, Doc};
use surco::figures::duration;
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::one_line;
use yunta_core::events::{gates, node, run};
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_replay::frame::{Happening, Moment};

use crate::blocks::FailureDetail;
use crate::command;
use words::carried;

/// One moment as a surface says it, before any layout decides where it
/// goes.
pub struct Said {
    /// What this moment is marked with, when it is about something
    /// reaching a state, asking a person or changing course; `None` for a
    /// moment that only reports.
    pub mark: Option<Mark>,
    pub text: String,
}

/// The words for `moment`. One match per domain, and the only ones.
pub fn say(moment: &Moment, glyphs: Glyphs) -> Said {
    let subject = match &moment.node {
        Some(node) => node.to_string(),
        None => "run".to_string(),
    };
    let (mark, carried) = carried(&moment.happening, glyphs);
    let text = joined(
        &subject,
        &one_line(&carried),
        of_its_subject(&moment.happening),
    );
    Said { mark, text }
}

/// The subject and its words as one line: what the subject did or became
/// reads after it as a sentence (`lint failed`), a remark about it after a
/// colon (`lint: called Read`), and words that open by naming the subject
/// name it once (`approve needs a decision`, not `approve gate `approve`
/// needs a decision`).
fn joined(subject: &str, carried: &str, predicate: bool) -> String {
    if carried.is_empty() {
        return subject.to_string();
    }
    if let Some(rest) = naming(subject, carried) {
        return format!("{subject} {rest}");
    }
    match predicate {
        true => format!("{subject} {carried}"),
        false => format!("{subject}: {carried}"),
    }
}

/// What follows the subject in words that open by naming it: `gate
/// `approve` …` for `approve`, `run `01K…` …` for the run.
fn naming<'a>(subject: &str, carried: &'a str) -> Option<&'a str> {
    if subject == "run" {
        let (_, rest) = carried.strip_prefix("run `")?.split_once("` ")?;
        return Some(rest);
    }
    ["node", "gate", "loop", "task"]
        .iter()
        .find_map(|kind| carried.strip_prefix(format!("{kind} `{subject}` ").as_str()))
}

/// Whether the moment's words say what its subject did or became: a run's
/// own moments, a node reaching a state, rerouted or opening its pull
/// request, and what a gate asks and is answered. The rest are remarks
/// about the subject.
fn of_its_subject(happening: &Happening) -> bool {
    match happening {
        Happening::Run(
            run::happening::Happening::HostSuspended { .. }
            | run::happening::Happening::BaselineCaptured { .. }
            | run::happening::Happening::BaselineHeld { .. }
            | run::happening::Happening::ChangesCounted { .. },
        ) => false,
        Happening::Run(_) => true,
        Happening::Node(
            node::happening::Happening::Reached { .. }
            | node::happening::Happening::Rerouted(_)
            | node::happening::Happening::PullRequestOpened { .. },
        ) => true,
        Happening::Gates(gates::happening::Happening::Resolved(_)) => false,
        Happening::Gates(_) => true,
        _ => false,
    }
}

/// Whether this moment says anything of its own. A node a person's choice sends back records
/// a failure on the log, since that is what reroutes it, and it is no failure: the reroute
/// that follows says where the choice sent the run, with what the person wrote.
pub fn says_anything(happening: &Happening) -> bool {
    !matches!(
        happening,
        Happening::Node(node::happening::Happening::Reached {
            state: yunta_core::events::NodeState::Failed {
                failure: yunta_core::events::Failure::GateChose { .. },
                ..
            },
            ..
        })
    )
}

/// The line one moment is said on, marked when it carries a mark.
pub fn line(moment: &Moment, look: &Look) -> Line {
    let said = say(moment, look.glyphs);
    let line = match said.mark {
        Some(mark) => Line::new()
            .push(Tone::of(mark), look.glyphs.mark(mark).to_string())
            .plain(" "),
        None => Line::new(),
    };
    line.plain(said.text)
}

/// The line one moment is logged on, for a reader who meets the run after the fact: the
/// run's own clock at that moment, in brackets, then the moment's line. What a pipe
/// writes, and what the run's chronicle shows wherever a person follows it.
pub fn logged(moment: &Moment, look: &Look) -> Line {
    Line::new()
        .plain(format!("[{}] ", duration(moment.elapsed)))
        .then(line(moment, look))
}

/// The run's chronicle as a document: every moment that says anything, logged, and
/// under a node that failed what it printed. `run` is what the run is called by.
pub fn told(moments: &[Moment], run: &RunId, look: &Look) -> Doc {
    let mut doc = Doc::new();
    let mut lines = Vec::new();
    for moment in moments
        .iter()
        .filter(|moment| says_anything(&moment.happening))
    {
        lines.push(logged(moment, look));
        let printed = evidence(moment, run);
        if !printed.is_empty() {
            doc = doc
                .with(Block::Lines(std::mem::take(&mut lines)))
                .with_all(printed);
        }
    }
    match lines.is_empty() {
        true => doc,
        false => doc.with(Block::Lines(lines)),
    }
}

/// What a node that failed printed, as the blocks a surface quotes under the
/// line that says it failed, and the command that shows all of it: the
/// reason is read where the failure is, not one command away. `run` is what
/// the run is called by. Nothing for any other moment.
pub fn evidence(moment: &Moment, run: &RunId) -> Vec<Block> {
    let Happening::Node(node::happening::Happening::Reached {
        state: yunta_core::events::NodeState::Failed { failure, .. },
        ..
    }) = &moment.happening
    else {
        return Vec::new();
    };
    let whole = failure.output().map(|_| {
        Whole::Command(match &moment.node {
            Some(node) => command::line(&NextStep::Status {
                run: run.clone(),
                node: Some(node.clone()),
                depth: Depth::Glance,
            }),
            None => command::line(&NextStep::status(run)),
        })
    });
    FailureDetail { failure, whole }.blocks()
}

#[cfg(test)]
mod tests;
