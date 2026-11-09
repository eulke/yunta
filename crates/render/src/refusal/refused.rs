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
use yunta_core::check::CheckError;
use yunta_core::diagnostic::Report;
use yunta_core::events::{Claim, FailureMenu, ObjectProblem, Refusal};
use yunta_core::inputs::InputsProblem;
use yunta_core::refusal::{AnswersRefusal, CloseRefusal, GateRefusal, Unservable};
use yunta_core::step::{Arg, NextStep, Reach};
use yunta_core::{InvalidId, Pid, RunId, UnknownDocument};
use yunta_replay::receipt::ReceiptError;
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
    /// `last`, where the repository it is asked in has no run.
    NoRunHere,
    /// `needs`, where no run of the repository it is asked in waits on a
    /// person.
    NoneNeedsYou,
    /// A text given where a run's id goes that no id can be, and the rule it
    /// breaks.
    InvalidId(InvalidId),
    /// A receipt asked of a run that has not finished, which has no closing
    /// figures to certify.
    NoReceipt(ReceiptError),
    /// A run in a repository git cannot commit in, which would fail at its
    /// first node's commit after spending on it: what git said.
    NoCommitter { git_said: String },
    /// A run that opens a pull request through a forge this machine cannot
    /// reach: the repository, and the variable its token is in, not set here.
    ForgeUnreachable { repo: String, token_env: String },
    /// What this binary cannot serve for the run.
    Unservable(Unservable),
    /// A run past `limits.max_concurrent_runs`: how many are still going, and
    /// the cap.
    TooManyRuns { active: usize, cap: u32 },
    /// A decision asked of a run that waits on none.
    NoDecision { run: RunId },
    /// What the workflow's own check refuses once a run's mode or the
    /// adapters this binary builds are known, under the workflow it judged.
    Unchecked {
        workflow: Arg<String>,
        errors: Vec<CheckError>,
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
/// could mean: the field's row a person reads, when the run reads back.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub run_id: RunId,
    pub row: Option<FieldRow>,
}

impl Candidate {
    /// The run `row` is.
    pub fn of(row: FieldRow) -> Self {
        Candidate {
            run_id: row.frame.run_id.clone(),
            row: Some(row),
        }
    }

    /// A run that does not read back, known by its id alone.
    pub fn unread(run_id: RunId) -> Self {
        Candidate { run_id, row: None }
    }

    /// What tells the run apart in words: the workflow it runs, and the mode
    /// it runs in.
    fn about(&self) -> Option<String> {
        let frame = &self.row.as_ref()?.frame;
        Some(format!("{} ({})", frame.workflow, frame.mode))
    }

    /// The run's row as the field draws it with `glyphs`, ending with the
    /// command that opens it.
    fn line(&self, glyphs: Glyphs) -> Option<RunLine> {
        let row = self.row.as_ref()?;
        let handle = self.run_id.handle().to_string();
        let opens = Edge::Command(command::line(&NextStep::status(&self.run_id)));
        Some(row.line(handle, opens, glyphs))
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

    /// The blocks a person's page draws under the refusal, with `glyphs`:
    /// each problem it found in a text, pointed at in its line, and what to
    /// declare for the ones a declaration fixes; or the runs a reference
    /// could mean. Empty for a refusal its sentence says whole.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        match self {
            Refused::AmbiguousRun { candidates, .. } => choice(candidates, glyphs),
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
            Refused::NoRunHere => super::words::no_run_here(),
            Refused::NoneNeedsYou => super::words::none_needs_you(),
            Refused::InvalidId(invalid) => invalid.to_string(),
            Refused::NoReceipt(error) => super::words::no_receipt(error),
            Refused::NoCommitter { git_said } => format!(
                "a run commits every node's work, and git {}",
                super::unnamed_committer(git_said)
            ),
            Refused::ForgeUnreachable { repo, token_env } => {
                super::words::forge_unreachable(repo, token_env)
            }
            Refused::Unservable(unservable) => super::words::unservable(unservable),
            Refused::TooManyRuns { active, cap } => super::words::too_many_runs(*active, *cap),
            Refused::NoDecision { run } => format!(
                "run {} waits on no decision; `{}` says where it stands",
                run.handle(),
                command::line(&NextStep::status(run))
            ),
            Refused::Unchecked { workflow, errors } => super::words::unchecked(workflow, errors),
            Refused::Gate { run, refusal } => format!(
                "{}{}",
                super::gate(refusal),
                gate_advice(run, reader, refusal)
            ),
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
    fn document(&self, look: &Look) -> Doc {
        let (detail, steps) = self.advice(Reader::Person);
        Doc::new()
            .with(components::Refusal {
                said: self.said(Reader::Person),
                detail,
            })
            .with_all(self.blocks(look.glyphs))
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
        .map(|candidate| match candidate.about() {
            Some(about) => format!("\n  {}  {about}", candidate.run_id),
            None => format!("\n  {}", candidate.run_id),
        })
        .collect()
}

/// The runs a reference could mean, as the rows a person chooses among: each
/// as the field shows it with `glyphs`, and one that does not read back by
/// its id.
fn choice(candidates: &[Candidate], glyphs: Glyphs) -> Vec<Block> {
    let rows: Vec<RunLine> = candidates
        .iter()
        .filter_map(|candidate| candidate.line(glyphs))
        .collect();
    let unread: Vec<String> = candidates
        .iter()
        .filter(|candidate| candidate.row.is_none())
        .map(|candidate| candidate.run_id.to_string())
        .collect();
    let rows = (!rows.is_empty()).then(|| Runs { rows }.into());
    let unread = (!unread.is_empty()).then(|| Said(unread).into());
    rows.into_iter().chain(unread).collect()
}

/// What to do instead of the decision a run refused, the run named the way
/// `reader` copies it: nothing beside a decision the refusal says whole.
fn gate_advice(run: &RunId, reader: Reader, refusal: &GateRefusal) -> String {
    match refusal {
        GateRefusal::NotPaused => format!(
            "; `{}` shows where it is",
            command::line_for(&NextStep::status(run), reader)
        ),
        GateRefusal::NothingToResolve => format!(
            "; settle it where it was raised, then `{}`",
            command::line_for(&NextStep::Resume { run: run.clone() }, reader)
        ),
        GateRefusal::UnknownOption { .. }
        | GateRefusal::Unsaid { .. }
        | GateRefusal::Withheld { .. } => String::new(),
    }
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
