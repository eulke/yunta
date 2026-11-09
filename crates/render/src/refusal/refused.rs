//! See [`super`]. One refusal as a value every interface says alike: its
//! sentence for a person and for an agent, where it looked, the commands that
//! go on from it, the blocks a person's page draws under it, and that page.

use std::fmt;
use std::path::PathBuf;

use surco::components::{self, Edge, Next, RunLine, Runs, Said};
use surco::doc::{Block, Doc};
use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::words::RUNS_THAT_NEED_YOU_FIRST;
use yunta_core::diagnostic::Report;
use yunta_core::events::{Claim, FailureMenu, ObjectProblem, Refusal};
use yunta_core::inputs::InputsProblem;
use yunta_core::refusal::{AnswersRefusal, CloseRefusal, GateRefusal};
use yunta_core::step::{Arg, NextStep, Reach};
use yunta_core::{Pid, RunId, UnknownDocument};
use yunta_replay::registry::Asking;

use crate::blocks::diagnostic::{counted_under, document_problems, plain};
use crate::command::{self, Reader};
use crate::page::{FieldRow, Page, Unrunnable};

/// What Yunta refused, with what a reader needs to say it: the run it is
/// about, where it looked, what a text it refused holds. Not a failure, which
/// something below broke; a refusal is what is true of a run, a request or a
/// workflow.
#[derive(Debug)]
pub enum Refused {
    /// A run this machine has no record of, wherever it looked: `roots`, as
    /// a person reads a path, and the run it was one slip away from.
    UnknownRun {
        run: RunId,
        roots: Vec<String>,
        near: Option<String>,
    },
    /// A reference several runs answer to, said as it was given, and every
    /// run it could be.
    AmbiguousRun {
        said: String,
        candidates: Vec<Candidate>,
    },
    /// A decision the run will not record.
    Gate { run: RunId, refusal: GateRefusal },
    /// A close the run will not make.
    Close { run: RunId, refusal: CloseRefusal },
    /// A reply to a node's questions the run will not take.
    Answers(AnswersRefusal),
    /// A run another engine drives, by its pid.
    Moving { run: RunId, pid: Pid },
    /// A run whose engine asks a person at its terminal, the one place that
    /// moves it.
    Asking(Asking),
    /// A run whose holder cannot be told from a leftover: a pid this user
    /// cannot signal, or a lock that names nobody. Only a person can tell.
    HoldUnclear {
        run: RunId,
        lock: PathBuf,
        pid: Option<Pid>,
    },
    /// Inputs a command was handed that its workflow does not take.
    Inputs(InputsProblem),
    /// A surface's answer to an escalation that is no decision on it, and the
    /// claim it was given for.
    SurfaceAnswer {
        refused: Refusal,
        claim: Box<Claim>,
        menu: Option<Box<FailureMenu>>,
    },
    /// An object a run's store does not hold, or holds other bytes under.
    Object(ObjectProblem),
    /// A document that does not read, or breaks its own rules, with the text
    /// it was read from when there is one.
    Document {
        report: Box<Report>,
        text: Option<String>,
    },
    /// A workflow that cannot run here.
    Unrunnable(Box<Unrunnable>),
    /// A document this binary does not show.
    UnknownDocument(UnknownDocument),
}

/// A run a reference could mean, and what tells it apart from the others it
/// could mean: in words, and as the field's row a person reads, when the run
/// reads back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub run_id: RunId,
    pub about: Option<String>,
    pub line: Option<RunLine>,
}

impl Candidate {
    /// The run `row` is, its row drawn with `glyphs` and ending with the
    /// command that opens it.
    pub fn of(row: &FieldRow, glyphs: Glyphs) -> Self {
        let frame = &row.frame;
        let handle = frame.run_id.handle().to_string();
        let opens = Edge::Command(command::line(&NextStep::status(&frame.run_id)));
        Candidate {
            run_id: frame.run_id.clone(),
            about: Some(format!("{} ({})", frame.workflow, frame.mode)),
            line: Some(row.line(handle, opens, glyphs)),
        }
    }

    /// A run that does not read back, known by its id alone.
    pub fn unread(run_id: RunId) -> Self {
        Candidate {
            run_id,
            about: None,
            line: None,
        }
    }
}

impl Refused {
    /// The refusal as `reader` reads it. A person whose page draws the
    /// problems or the runs under it reads how many there are, or that a run
    /// has to be named; any other reading says the refusal whole, a run named
    /// by its handle for a person and by its whole id for an agent.
    pub fn said(&self, reader: Reader) -> String {
        match (reader, self.headline_for_a_person()) {
            (Reader::Person, Some(headline)) => headline,
            _ => self.sentence(reader),
        }
    }

    /// What `reader` is told beside the refusal: where it looked, and the
    /// commands that go on from here. Empty for a refusal that says it all
    /// in its sentence.
    pub fn advice(&self, reader: Reader) -> (Option<String>, Vec<(String, &'static str)>) {
        match self {
            Refused::Moving { run, .. } => (
                None,
                vec![
                    (
                        command::line_for(&NextStep::Follow { run: run.clone() }, reader),
                        "follows it",
                    ),
                    (
                        command::line_for(&NextStep::Cancel { run: run.clone() }, reader),
                        "stops it",
                    ),
                ],
            ),
            Refused::UnknownRun { roots, .. } => (
                Some(format!("looked in {}", roots.join(" and "))),
                vec![
                    (
                        command::line(&NextStep::ListRuns {
                            reach: Reach::Repository,
                        }),
                        RUNS_THAT_NEED_YOU_FIRST,
                    ),
                    (
                        command::line(&NextStep::ListRuns {
                            reach: Reach::Machine,
                        }),
                        "every run on this machine",
                    ),
                ],
            ),
            _ => (None, Vec::new()),
        }
    }

    /// The blocks a person's page draws under the refusal: each problem it
    /// found in a text, pointed at in its line, and what to declare for the
    /// ones a declaration fixes; or the runs a reference could mean. Empty
    /// for a refusal its sentence says whole.
    pub fn blocks(&self) -> Vec<Block> {
        match self {
            Refused::AmbiguousRun { candidates, .. } => choice(candidates),
            Refused::Document { report, text } => document_problems(report, text.as_deref())
                .into_iter()
                .map(Block::from)
                .collect(),
            Refused::Unrunnable(refusal) => refusal.blocks(&[]),
            _ => Vec::new(),
        }
    }

    /// The sentence a person's page opens with when the problems or the runs
    /// it names are blocks under it. `None` for any other refusal.
    fn headline_for_a_person(&self) -> Option<String> {
        match self {
            Refused::Document { report, .. } => Some(counted_under(
                &report.document.path,
                report.diagnostics.len(),
            )),
            Refused::Unrunnable(refusal) => Some(counted_under(
                &super::unrunnable(Arg::Given(refusal.workflow.clone())),
                refusal.problems.len(),
            )),
            Refused::AmbiguousRun { said, .. } => Some(name_one(said)),
            _ => None,
        }
    }

    /// The refusal's whole sentence, any run it names named the way `reader`
    /// copies it.
    fn sentence(&self, reader: Reader) -> String {
        let named = |run: &RunId| reader.name_of(run).to_string();
        match self {
            Refused::UnknownRun { run, near, .. } => format!(
                "no run is called `{run}`{}",
                surco::text::near_miss(near.as_deref())
            ),
            Refused::AmbiguousRun { said, candidates } => {
                format!("{}:{}", name_one(said), one_per_line(candidates))
            }
            Refused::Gate { run, refusal } => match refusal {
                GateRefusal::NotPaused => format!(
                    "{}; `{}` shows where it is",
                    super::gate(refusal),
                    command::line_for(&NextStep::status(run), reader)
                ),
                GateRefusal::NothingToResolve => format!(
                    "{}; settle it where it was raised, then `{}`",
                    super::gate(refusal),
                    command::line_for(&NextStep::Resume { run: run.clone() }, reader)
                ),
                GateRefusal::UnknownOption { .. }
                | GateRefusal::Unsaid { .. }
                | GateRefusal::Withheld { .. } => super::gate(refusal),
            },
            Refused::Close { run, refusal } => format!(
                "{}{}",
                super::close(refusal),
                close_advice(run, reader, refusal)
            ),
            Refused::Answers(refusal) => super::answers(refusal),
            Refused::Moving { run, pid } => super::moving(&named(run), *pid),
            Refused::Asking(asking) => crate::engine::asking_claim(asking),
            Refused::HoldUnclear { run, lock, pid } => super::hold_unclear(&named(run), lock, *pid),
            Refused::Inputs(problem) => super::inputs(problem),
            Refused::SurfaceAnswer {
                refused,
                claim,
                menu,
            } => super::refused_answer(refused, claim, menu.as_deref()),
            Refused::Object(problem) => crate::failure::object(problem),
            Refused::Document { report, text } => plain(
                &report.document.path,
                document_problems(report, text.as_deref()),
            ),
            Refused::Unrunnable(refusal) => unrunnable_said(refusal),
            Refused::UnknownDocument(unknown) => unknown.to_string(),
        }
    }
}

/// The refusal as a person reads it whole, on one line or several.
impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.sentence(Reader::Person))
    }
}

impl std::error::Error for Refused {}

/// A refusal as a person reads it on stderr: the sentence and where it
/// looked, each problem it found pointed at in its line, and the commands
/// that go on from here.
impl Page for Refused {
    fn document(&self, _look: &Look) -> Doc {
        let (detail, steps) = self.advice(Reader::Person);
        Doc::new()
            .with(components::Refusal {
                said: self.said(Reader::Person),
                detail,
            })
            .with_all(self.blocks())
            .with(Next { steps })
    }
}

/// A sentence that several runs answer to, asking for one of them.
fn name_one(said: &str) -> String {
    format!("{said}; name one")
}

/// Every run a reference could mean, one to a line under the sentence: the
/// id whole, and what tells it apart when there is something to say.
fn one_per_line(candidates: &[Candidate]) -> String {
    candidates
        .iter()
        .map(|candidate| match &candidate.about {
            Some(about) => format!("\n  {}  {about}", candidate.run_id),
            None => format!("\n  {}", candidate.run_id),
        })
        .collect()
}

/// The runs a reference could mean, as the rows a person chooses among: each
/// as the field shows it, and one that does not read back by its id.
fn choice(candidates: &[Candidate]) -> Vec<Block> {
    let rows: Vec<RunLine> = candidates
        .iter()
        .filter_map(|candidate| candidate.line.clone())
        .collect();
    let unread: Vec<String> = candidates
        .iter()
        .filter(|candidate| candidate.line.is_none())
        .map(|candidate| candidate.run_id.to_string())
        .collect();
    let rows = (!rows.is_empty()).then(|| Runs { rows }.into());
    let unread = (!unread.is_empty()).then(|| Said(unread).into());
    rows.into_iter().chain(unread).collect()
}

/// What to do instead of closing a run that refused it, the run named the way
/// `reader` copies it.
fn close_advice(run: &RunId, reader: Reader, refusal: &CloseRefusal) -> String {
    match refusal {
        CloseRefusal::Driven => format!(
            "; `{}` stops it, and then it can be closed",
            command::line_for(&NextStep::Cancel { run: run.clone() }, reader)
        ),
        CloseRefusal::Moving => format!(
            "; `{}` shows where it is",
            command::line_for(&NextStep::status(run), reader)
        ),
        CloseRefusal::AlreadyClosed => String::new(),
    }
}

/// A workflow that cannot run here, as plain text says it: its problems
/// counted and quoted, then what to declare.
fn unrunnable_said(refusal: &Unrunnable) -> String {
    use surco::width::INDENT;
    let mut said = plain(
        &super::unrunnable(Arg::Given(refusal.workflow.clone())),
        refusal.problems.clone(),
    );
    for fix in &refusal.declare {
        said.push_str(&format!("\n{INDENT}{}", fix.said));
        for line in &fix.paste {
            said.push_str(&format!("\n{INDENT}{INDENT}{INDENT}{line}"));
        }
    }
    said
}
