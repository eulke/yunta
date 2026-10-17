//! Where a run stands, drawn as one document: the page `yunta status`
//! prints for a run that may be moving, and the block a run leaves on
//! the terminal once it has stopped.
//!
//! A document reads in the order a person reads it for: the word a run is
//! called by and what holds it, the evidence of what failed, the decision a
//! parked run waits on, the facts that have something to say, and what to
//! type next. A section with nothing to say is not a section. Nothing here
//! paints: the caller draws the document once, with the look of its stream.
//!
//! The two pages read the same frame and say different things about it. A
//! stopped run says how it ended; a run that may be moving says where
//! it stands. Each page's sections live beside the command that shows it,
//! and this module holds what both share.

use yunta_core::caution::Caution;
use yunta_core::events::{Failure, GateWaitingPayload, NodeState, PauseReason};
use yunta_core::RunId;
use yunta_engine::{NodeFrame, NodeStanding, RunFrame, RunPhase, RunState, WaitingOn};
use yunta_render::spend::Breakdown;

use crate::commands::advice;
use crate::render::blocks::{Drawn, Headline};
use crate::render::doc::{Block, Doc};
use crate::render::ink::{Line, Tone};
use crate::render::state::RunWord;
use crate::render::{truncate, wrap, Look, INDENT};
use crate::surface::Outline;

/// Which page a run is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The block a run leaves on the terminal once it has stopped.
    Closing,
    /// The page `yunta status` prints for a run.
    Status,
}

/// The decision a parked run waits on, as a page draws it.
#[derive(Debug, Clone)]
pub(crate) struct Decided {
    /// The decision as lines, laid out for the look the page is drawn with.
    pub(crate) lines: Vec<Line>,
    /// The escalation the run stopped on, when the log rebuilds one. The
    /// commands that answer it depend on what it is about.
    pub(crate) escalation: Option<GateWaitingPayload>,
}

/// What a page is drawn from: one run as its log derives it, the places its
/// page names, and the decision it waits on.
pub(crate) struct RunPage<'a> {
    pub(crate) scope: Scope,
    pub(crate) run_id: &'a RunId,
    /// What the run's registry says about the process driving it.
    pub(crate) engine: crate::render::Engine,
    pub(crate) frame: &'a RunFrame,
    /// The run as its log derives it; the frame is read from the same log.
    pub(crate) state: &'a RunState,
    /// Where the page's paths are shown from, and the tree the run works in.
    pub(crate) paths: Outline<'a>,
    /// The menu a parked run waits on.
    pub(crate) decision: Option<Decided>,
}

impl RunPage<'_> {
    /// The page as one document. Nothing here paints: `render::draw` does.
    pub(crate) fn document(&self, look: &Look) -> Doc<'_> {
        match self.scope {
            Scope::Closing => self.closing_document(look),
            Scope::Status => self.status_document(look),
        }
    }

    /// The block a stopped run leaves on the terminal.
    fn closing_document(&self, look: &Look) -> Doc<'_> {
        let word = crate::render::run_word(self.frame);
        let doc = Doc::new()
            .with(Headline {
                subject: format!("run {}", self.run_id.handle()),
                mark: word.mark(),
                said: word.to_string(),
            })
            .with(Block::Lines(self.closing_holds(look)));
        let doc = section(doc, self.closing_evidence(look));
        let doc = section(doc, self.decided_lines());
        let doc = section(doc, self.closing_facts(look));
        section(doc, self.closing_next().lines(look))
    }

    /// The page `yunta status` prints for a run.
    fn status_document(&self, look: &Look) -> Doc<'_> {
        let word = crate::render::observed_word(self.frame, &self.engine);
        let doc = Doc::new()
            .with(Headline {
                subject: format!("run {}", self.run_id),
                mark: word.mark(),
                said: word.word().to_string(),
            })
            .with(Block::Lines(self.status_holds(word, look)));
        let mut nodes = self.status_nodes(look);
        nodes.extend(self.status_calls());
        let doc = section(doc, nodes);
        let doc = section(doc, self.status_evidence(look));
        let doc = section(doc, self.status_fields(look.glyphs).lines(look));
        let doc = section(doc, self.decided_lines());
        section(doc, self.status_next().lines(look))
    }

    /// The decision a parked run waits on, as the lines a page places.
    fn decided_lines(&self) -> Vec<Line> {
        self.decision
            .as_ref()
            .map(|decided| decided.lines.clone())
            .unwrap_or_default()
    }

    /// The escalation the run stopped on, when the log rebuilds one.
    pub(crate) fn decided_escalation(&self) -> Option<&GateWaitingPayload> {
        self.decision
            .as_ref()
            .and_then(|decided| decided.escalation.as_ref())
    }

    /// Settled and in-flight spend, split in and out: nothing for a run that
    /// spent none. Every page reads this one figure, from the frame the run's
    /// log derives.
    pub(crate) fn tokens(&self) -> Option<String> {
        let spend = &self.frame.tokens;
        (spend.total() > 0).then(|| Breakdown(spend).to_string())
    }

    /// What holds a stopped run, under its headline: its own sentence where
    /// its word leaves that unsaid, wrapped to the line.
    fn closing_holds(&self, look: &Look) -> Vec<Line> {
        let room = look.width.cells().saturating_sub(INDENT.len());
        match self.closing_said() {
            Some((tone, said)) => wrap(&said, room)
                .into_iter()
                .map(|part| Line::new().plain(INDENT).push(tone, part))
                .collect(),
            None => Vec::new(),
        }
    }

    /// The tone and sentence of what holds a stopped run. `None` when its
    /// word already says it.
    pub(crate) fn closing_said(&self) -> Option<(Tone, String)> {
        let blocking = self.frame.blocking_findings;
        Some(match &self.frame.phase {
            // A limit's reset moves the run on its own, so the wait is not a
            // stop a person is asked to answer.
            RunPhase::Waiting { on } if advice::reset_pause(&self.frame.phase).is_some() => {
                (Tone::Muted, advice::parked_in_full(on))
            }
            RunPhase::Waiting { on } => (Tone::NeedsYou, advice::parked_in_full(on)),
            RunPhase::Failed {
                failure: Some(failure),
            } => (Tone::Failed, self.failed_claim(failure)),
            RunPhase::Broken { diagnostic } => {
                (Tone::Failed, yunta_core::text::one_line(diagnostic))
            }
            // The work is done and nobody has accepted it.
            RunPhase::Finished if blocking > 0 => (
                Tone::Caution,
                format!(
                    "finished holding {}",
                    yunta_core::text::counted(blocking, "blocking finding")
                ),
            ),
            RunPhase::Promoted { to: Some(mode) } => (Tone::Muted, format!("promoted to `{mode}`")),
            // A run this block is drawn over that has not stopped is
            // reported as what it is doing, not as a stop it has not
            // reached.
            RunPhase::Created | RunPhase::Running => (Tone::Muted, "still moving".to_string()),
            RunPhase::Finished
            | RunPhase::Failed { failure: None }
            | RunPhase::Cancelled
            | RunPhase::Promoted { to: None } => return None,
        })
    }

    /// The failure that closed the run, said as the node it happened to.
    fn failed_claim(&self, failure: &Failure) -> String {
        let headline = yunta_render::failure::headline(failure);
        match failed(self.frame).find(|(_, found)| *found == failure) {
            Some((node, _)) => format!("node `{}` failed: {headline}", node.id),
            None => headline,
        }
    }

    /// What holds a run that may be moving, on the line under its
    /// headline. The word is the one the page calls the run by.
    fn status_holds(&self, word: RunWord, look: &Look) -> Vec<Line> {
        let (tone, said) = self.status_said(word);
        let room = look.width.cells().saturating_sub(INDENT.len());
        vec![Line::new()
            .plain(INDENT)
            .push(tone, truncate(&said, room, look.glyphs).trim_end())]
    }

    /// The tone and sentence of what holds a run that may be moving,
    /// or that nothing does.
    fn status_said(&self, word: RunWord) -> (Tone, String) {
        match (word, &self.frame.phase) {
            (RunWord::Stalled, _) => (Tone::Caution, advice::STALLED.to_string()),
            (RunWord::NeedsYou, RunPhase::Created | RunPhase::Running) => {
                match &self.engine.prompt {
                    Some(prompt) => (Tone::NeedsYou, advice::asking(prompt, self.engine.at)),
                    None => (Tone::NeedsYou, "it needs you".to_string()),
                }
            }
            // A limit's reset moves the run on its own, so the wait is not a
            // stop a person is asked to answer.
            (RunWord::Waiting, RunPhase::Waiting { on }) => {
                (Tone::Muted, advice::parked_in_full(on))
            }
            (_, RunPhase::Waiting { on }) => (Tone::NeedsYou, advice::parked_in_full(on)),
            (_, RunPhase::Broken { diagnostic }) => {
                (Tone::Failed, yunta_core::text::one_line(diagnostic))
            }
            (
                _,
                RunPhase::Failed {
                    failure: Some(failure),
                },
            ) => (
                Tone::Failed,
                yunta_core::text::one_line(&crate::render::failure::sentence(failure)),
            ),
            (_, RunPhase::Created | RunPhase::Running) => {
                (Tone::Muted, "nothing needs you: it is moving".to_string())
            }
            _ => (Tone::Muted, "nothing needs you".to_string()),
        }
    }
}

/// The nodes that stand failed, each with its failure, in declaration order.
pub(crate) fn failed(frame: &RunFrame) -> impl Iterator<Item = (&NodeFrame, &Failure)> {
    frame.nodes.iter().filter_map(|node| match &node.state {
        NodeStanding::Reached(NodeState::Failed { failure, .. }) => Some((node, failure)),
        _ => None,
    })
}

/// The cautions a page says beside its facts. A run parked on its token
/// budget says what the declared cap means against what this workflow's
/// past runs spent; a run parked on anything else says none of it.
pub(crate) fn cautions(frame: &RunFrame) -> Vec<Caution> {
    match &frame.phase {
        RunPhase::Waiting {
            on:
                WaitingOn::Run {
                    reason: PauseReason::BudgetExhausted { cap, .. },
                },
        } => yunta_engine::budget_caution(Some(*cap), frame.prior.as_ref())
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// The cautions of a run, each in the words render gives it, as one row.
pub(crate) fn caution_row(frame: &RunFrame) -> String {
    cautions(frame)
        .iter()
        .map(yunta_render::caution::line)
        .collect::<Vec<_>>()
        .join("; ")
}

/// `lines` as a section of a document: a blank line above it, and nothing at
/// all when there is nothing to say.
pub(crate) fn section(doc: Doc<'_>, lines: Vec<Line>) -> Doc<'_> {
    if lines.is_empty() {
        return doc;
    }
    let mut block = vec![Line::new()];
    block.extend(lines);
    doc.with(Block::Lines(block))
}

#[cfg(test)]
mod tests;
