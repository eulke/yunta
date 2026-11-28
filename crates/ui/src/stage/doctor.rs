//! See [`super`]. The machine: what doctor checks of this machine and this project, opened
//! from the field with `m`: every check found, as `yunta doctor` prints it, folded at a
//! glance or, with `f`, every check on its own row, as `--full` does, under a headline that
//! names this machine. `s` opens a session per binding, as `--session` does, once a person
//! confirms what it spends: what the sessions remark stands as it comes, above the page, as
//! `yunta doctor` says it before its page, and Escape stops them. `u` opens upkeep, which a
//! checkouts finding over its budget leads to, and doctor checks again when the person
//! comes back, what a round of sessions found standing under what the checks find until
//! sessions are asked again. A page taller than what shows of it scrolls. It stands in the frame of the
//! field it was opened over (`over_the_field`), where `a` goes where its attention row
//! names. `n` opens a new run over it, Escape goes back to the runs and `q` goes down to the
//! field.

use chrono::{DateTime, Utc};
use surco::components::{Laid, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::Remark;
use yunta_core::step::{Arg, NextStep};
use yunta_render::page::doctor::{Doctor, Finding};
use yunta_render::screen;

use super::over_the_field::{attending_over_the_field, in_the_fields_frame, KeyA};
use super::{Act, Want};
use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{FollowedRuns, Operation, Outcome};

/// What doctor found of this machine and this project, and the checks under way.
#[derive(Debug, Clone)]
pub struct DoctorStage {
    /// What the last checks found, once they have.
    found: Option<Found>,
    /// The checks under way, where they are.
    running: Option<Running>,
    /// Whether every check stands on its own row.
    full: bool,
    /// What the checks under way, or the last ones, remarked.
    remarked: Vec<Remark>,
    /// Where the page is scrolled to.
    scroll: Scroll,
    /// What refused, failed or was stopped, where something was.
    said: Option<String>,
}

/// What a round of checks found, and whether it opened a session per binding.
#[derive(Debug, Clone)]
struct Found {
    sessions: bool,
    findings: Vec<Finding>,
}

/// Which checks are under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Running {
    /// Every check of the machine and the project.
    Checks,
    /// Every check, and one real session per binding.
    Sessions,
}

impl DoctorStage {
    /// Doctor, its checks yet to run.
    pub(crate) fn new() -> Self {
        DoctorStage {
            found: None,
            running: Some(Running::Checks),
            full: false,
            remarked: Vec::new(),
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: every check, with no session.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![Want::Work(Operation::Doctor { sessions: false })]
    }

    /// The work its screen says is under way: every check, or a session per binding beside
    /// them.
    pub(crate) fn shown_under_way(&self) -> Option<Operation> {
        let sessions = |running| running == Running::Sessions;
        (self.running).map(|running| Operation::Doctor {
            sessions: sessions(running),
        })
    }

    /// What the stage asks for when a person comes back to it, from upkeep: every
    /// check again, what was found standing until they end; nothing while its sessions
    /// run, which come to their own findings.
    pub(crate) fn returned(&mut self) -> Vec<Want> {
        if self.running == Some(Running::Sessions) {
            return Vec::new();
        }
        self.running = Some(Running::Checks);
        self.said = None;
        self.opening()
    }

    /// What the checks remarked, then what they found, as the screen tells doctor's page, or
    /// that they run; what refused, failed or was stopped; in the frame of the field
    /// `followed` keeps at `now`, a line typed in what stands over it where `typed`; with the
    /// keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let mut doc = self.remarked.iter().fold(Doc::new(), |doc, remark| {
            doc.with(Note(crate::remark::said(remark, look.glyphs).words))
        });
        if let Some(found) = &self.found {
            let page = Doctor {
                session: found.sessions,
                findings: &found.findings,
                full: self.full,
            };
            doc = doc.with_all(page.on_screen().into_blocks());
        }
        let running = match self.running {
            Some(Running::Checks) => Some(screen::CHECKING_THE_MACHINE),
            Some(Running::Sessions) => Some(screen::OPENING_SESSIONS),
            None => None,
        };
        if let Some(running) = running {
            doc = doc.with(Note(running.to_string()));
        }
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut shown = Screen::new(Region::of_a_page(doc).in_sight(self.scroll.sight()));
        shown.trail = vec![screen::YUNTA.to_string(), screen::MACHINE.to_string()];
        let checks = match self.running {
            None => screen::DoctorChecks::Ended,
            Some(Running::Checks) => screen::DoctorChecks::Running,
            Some(Running::Sessions) => screen::DoctorChecks::OpeningSessions,
        };
        shown.legend = screen::doctor_legend(self.full, checks);
        in_the_fields_frame(&mut shown, followed, now, typed, KeyA::Attends);
        shown
    }

    /// What `stroke` asks for here, over what `followed` keeps at `now`. While the sessions
    /// run, Escape stops them. Otherwise `f` shows every check or folds them, `s` asks to
    /// confirm a session per binding, `u` opens upkeep, and Escape goes back. `a` goes where
    /// the attention row names, `n` opens a new run over the page, `q` goes down to the field,
    /// Ctrl-C leaves, and the keys that scroll scroll the page.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        let sessions = self.running == Some(Running::Sessions);
        match stroke {
            Stroke::Decline if sessions => {
                self.running = None;
                self.said = Some(screen::SESSIONS_STOPPED.to_string());
                vec![Want::LetGo(Operation::Doctor { sessions: true })]
            }
            Stroke::Decline => vec![Want::Back],
            Stroke::Insert('q') => vec![Want::ToTheField(None)],
            Stroke::Interrupt => vec![Want::Leave],
            Stroke::Insert('f') if !sessions => {
                self.full = !self.full;
                Vec::new()
            }
            Stroke::Insert('s') if self.running.is_none() => {
                let sessions = Operation::Doctor { sessions: true };
                let same = NextStep::Doctor {
                    sessions: true,
                    full: false,
                };
                let asks = screen::Confirming::OpenSessions;
                vec![Want::Confirm(Confirmation::new(asks, vec![same], sessions))]
            }
            Stroke::Insert('u') if !sessions => Want::taking(&NextStep::Gc { dry_run: true }),
            Stroke::Insert('a') => attending_over_the_field(followed, now),
            Stroke::Insert('n') if followed.can_ask_for_a_run() => Want::taking(&NextStep::Run {
                workflow: Arg::Blank,
            }),
            other => {
                self.scroll.pressed(other);
                Vec::new()
            }
        }
    }

    /// Whether a session per binding is open, its checks under way: where `f` and `u` wait.
    pub(crate) fn sessions_open(&self) -> bool {
        self.running == Some(Running::Sessions)
    }

    /// What doctor offers, each with the key that takes it here: a session per binding,
    /// every check or the glance again, and upkeep; nothing while the sessions run.
    pub(crate) fn offered(&self) -> Vec<Act> {
        if self.running == Some(Running::Sessions) {
            return Vec::new();
        }
        let sessions = NextStep::Doctor {
            sessions: true,
            full: false,
        };
        let rows = NextStep::Doctor {
            sessions: false,
            full: !self.full,
        };
        vec![
            Act::by(Stroke::Insert('s'), sessions),
            Act::by(Stroke::Insert('f'), rows),
            Act::by(Stroke::Insert('u'), NextStep::Gc { dry_run: true }),
        ]
    }

    /// What an act the stage asked to be confirmed does once it is: a session per
    /// binding opens, and what it remarks stands as it comes.
    pub(crate) fn confirmed(&mut self, operation: Operation) -> Vec<Want> {
        if operation == (Operation::Doctor { sessions: true }) {
            self.running = Some(Running::Sessions);
            self.remarked.clear();
            self.said = None;
        }
        vec![Want::Work(operation)]
    }

    /// What the checks under way remark, which stands as it comes.
    pub(crate) fn remarked(&mut self, remark: Remark) {
        self.remarked.push(remark);
    }

    /// What checks found, a session per binding opened with them where `sessions`: what
    /// they found; plain checks after a round of sessions keep what that round's sessions
    /// found, which stands until the person asks for sessions again or leaves doctor.
    fn refreshed(&self, sessions: bool, mut findings: Vec<Finding>) -> Found {
        let kept: Vec<Finding> = match &self.found {
            Some(found) if found.sessions && !sessions => (found.findings.iter())
                .filter(|finding| finding.of_a_session())
                .cloned()
                .collect(),
            Some(_) | None => Vec::new(),
        };
        let sessions = sessions || !kept.is_empty();
        findings.extend(kept);
        Found { sessions, findings }
    }

    /// Where its page stands, which no row of its screen says, its legend the same whether
    /// it scrolls or not.
    pub(crate) fn page(&self) -> Scroll {
        self.scroll
    }

    /// What a surface laid out of the page, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// What an answer to the checks does: what they found stands, what a round of sessions
    /// found kept under what plain checks found again; what refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        let sessions = self.running == Some(Running::Sessions);
        match outcome {
            Outcome::Doctor(findings) => {
                self.found = Some(self.refreshed(sessions, findings));
                self.running = None;
            }
            Outcome::Refused(refused) => {
                self.running = None;
                self.said = Some(refused.to_string());
            }
            Outcome::Failed(said) => {
                self.running = None;
                self.said = Some(said);
            }
            Outcome::Repository(_)
            | Outcome::Detected { .. }
            | Outcome::SetupPack { .. }
            | Outcome::Initialized { .. }
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::PullRequest(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Collected(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Stopped => {}
        }
        Vec::new()
    }
}
