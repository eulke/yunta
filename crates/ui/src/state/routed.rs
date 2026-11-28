//! See [`super`]. Who asked for each piece of work, and where what it hands back goes.
//!
//! Each stage that stands is someone: the state hands it an identity as it comes to stand
//! ([`Standing`]), first, opened over another, in another's place or at the bottom in the
//! place of what stood there, and it keeps it wherever it then stands, on screen or under
//! another. Each piece of work the state waits on keeps who asked it ([`Waited`]): the
//! application as it opens, or the stages that asked it, more than one where another stage
//! asks the same work while it is under way, so work is never asked twice. What the work hands
//! back, an answer, a remark or news, goes to each stage that asked it and still stands,
//! wherever it stands: a page a stage was opened over takes its own answers under it, and
//! nothing it waited on is asked again for it as it comes back. A stage under the one the
//! person is at carries
//! out what it wants back as work alone, asked, let go or released: what moves the person is
//! the stage on screen's.
//!
//! A stage leaves for good as the person goes back from it, down past it, or another takes
//! its place: the readings it alone waited on are let go and what the performer holds for it
//! released, while its acts go on. An act whose every asker left is noted where the person is
//! as it ends ([`Stage::noted`]): the stage on screen says only what a person should see there
//! of work it never asked. A reading asked again by a stage supersedes the reading of its kind
//! that stage waits on, and never another stage's, whose page waits on it.
//!
//! Not position: two stages at the same depth, one after the other, are two stages.

use super::State;
use crate::stage::Want;
use crate::{News, Operation, Outcome, Request, Stage};
use yunta_api::Remark;

/// Who a stage is among those that stood since the application opened: what the work it
/// asked keeps of it, so its answers find it wherever it stands. Not its place in what stands,
/// which another stage takes once it is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StageId(u64);

impl StageId {
    /// The identity the first stage to stand takes.
    pub(super) fn first() -> StageId {
        StageId(1)
    }
}

/// A stage as it stands, with who it is.
#[derive(Debug, Clone)]
pub(super) struct Standing {
    pub(super) id: StageId,
    pub(super) stage: Stage,
}

/// Who asked for a piece of work: the application, which reads where it stands as it opens,
/// or a stage, by who it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Asker {
    TheApplication,
    Stage(StageId),
}

/// A piece of work the state waits on: what it is, and each who asked it and has not left. An
/// act whose askers all left goes on with none.
#[derive(Debug, Clone)]
pub(super) struct Waited {
    pub(super) operation: Operation,
    pub(super) askers: Vec<Asker>,
}

impl State {
    /// `stage` standing as someone new: the identity it takes.
    pub(super) fn stood(&mut self, stage: Stage) -> Standing {
        let id = self.next_stage;
        self.next_stage = StageId(id.0 + 1);
        Standing { id, stage }
    }

    /// The stage `asker` names, where it stands, on screen or under another.
    fn standing_mut(&mut self, asker: Asker) -> Option<&mut Stage> {
        let Asker::Stage(id) = asker else {
            return None;
        };
        let under = self.under.iter_mut();
        let mut standing = self.stage.iter_mut().chain(under);
        standing
            .find(|standing| standing.id == id)
            .map(|standing| &mut standing.stage)
    }

    /// Whether `asker` stands: the application always does; a stage until it leaves.
    fn stands(&self, asker: Asker) -> bool {
        match asker {
            Asker::TheApplication => true,
            Asker::Stage(id) => self
                .stage
                .iter()
                .chain(&self.under)
                .any(|standing| standing.id == id),
        }
    }

    /// Who the person is at: the stage on screen, or the application before any stands.
    pub(super) fn on_screen(&self) -> Asker {
        match &self.stage {
            Some(standing) => Asker::Stage(standing.id),
            None => Asker::TheApplication,
        }
    }

    /// `operation` asked for by `by`, under a ticket of its own, which the state then waits
    /// on: nothing more where the same work is under way already, which `by` joins the askers
    /// of; for a reading, the one of its kind `by` waits on superseded first.
    pub(super) fn asked(&mut self, operation: Operation, by: Asker) -> Vec<Request> {
        let same = self
            .waiting
            .values_mut()
            .find(|waited| waited.operation == operation);
        if let Some(waited) = same {
            if !waited.askers.contains(&by) {
                waited.askers.push(by);
            }
            return Vec::new();
        }
        let kind = operation.kind();
        // A reading supersedes the reading of its kind its asker waits on, never an act of it
        // under way, checking doctor again leaving its sessions running, nor another stage's.
        let mut asked = match operation.reads() {
            true => self.let_go(by, |waited| waited.kind() == kind && waited.reads()),
            false => Vec::new(),
        };
        let ticket = self.next;
        self.next = ticket.next();
        asked.push(Request::Perform(ticket, operation.clone()));
        let askers = vec![by];
        self.waiting.insert(ticket, Waited { operation, askers });
        asked
    }

    /// Each piece of work `by` asked that `which` picks no longer `by`'s: let go, no longer
    /// waited on and stopped where it is under way, where no one else asked it.
    pub(super) fn let_go(&mut self, by: Asker, which: impl Fn(&Operation) -> bool) -> Vec<Request> {
        let mut let_go = Vec::new();
        self.waiting.retain(|ticket, waited| {
            if !which(&waited.operation) || !waited.askers.contains(&by) {
                return true;
            }
            waited.askers.retain(|asker| *asker != by);
            let kept = !waited.askers.is_empty();
            if !kept {
                let_go.push(Request::LetGo(*ticket));
            }
            kept
        });
        let_go
    }

    /// `left`, a stage leaving for good: what the performer holds for it released, and the
    /// readings it alone waited on let go; every piece of work it asked no longer its, its
    /// acts going on.
    pub(super) fn gone(&mut self, left: &Standing) -> Vec<Request> {
        let mut asked: Vec<Request> = left.stage.held().map(Request::LetGo).into_iter().collect();
        let by = Asker::Stage(left.id);
        asked.extend(self.let_go(by, Operation::reads));
        for waited in self.waiting.values_mut() {
            waited.askers.retain(|asker| *asker != by);
        }
        asked
    }

    /// What `outcome`, the answer to the work `waited` was, does: handed to each who asked it
    /// and still stands, wherever it stands, what each wants back carried out; where all left,
    /// noted where the person is.
    pub(super) fn answer_handed(&mut self, waited: Waited, outcome: Outcome) -> Vec<Request> {
        let Waited { operation, askers } = waited;
        let askers: Vec<Asker> = askers
            .into_iter()
            .filter(|asker| self.stands(*asker))
            .collect();
        let Some((last, before)) = askers.split_last() else {
            if let Some(standing) = self.stage.as_mut() {
                standing.stage.noted(&outcome);
            }
            return Vec::new();
        };
        let mut asked = Vec::new();
        for asker in before {
            asked.extend(self.answered_to(*asker, &operation, outcome.clone()));
        }
        asked.extend(self.answered_to(*last, &operation, outcome));
        asked
    }

    /// What `outcome`, the answer to `operation`, does handed to `asker`: the application's
    /// own reading, or what the stage makes of it and wants back.
    fn answered_to(
        &mut self,
        asker: Asker,
        operation: &Operation,
        outcome: Outcome,
    ) -> Vec<Request> {
        let wants: Vec<Want> = match asker {
            Asker::TheApplication => return self.opened_answered(outcome),
            Asker::Stage(_) => match self.standing_mut(asker) {
                Some(stage) => stage.answered(operation, outcome),
                None => Vec::new(),
            },
        };
        self.wanted(asker, wants)
    }

    /// What `remark`, said by the work `waited` is, does: kept by each stage that asked it and
    /// still stands.
    pub(super) fn remark_handed(&mut self, waited: &Waited, remark: Remark) {
        for asker in waited.askers.clone() {
            if let Some(stage) = self.standing_mut(asker) {
                stage.remarked(remark.clone());
            }
        }
    }

    /// What `news`, handed by the work `waited` is while it lasts, does: heard by each stage
    /// that asked it and still stands, what each wants back carried out.
    pub(super) fn news_handed(&mut self, waited: &Waited, news: News) -> Vec<Request> {
        let mut asked = Vec::new();
        for asker in waited.askers.clone() {
            let wants = match self.standing_mut(asker) {
                Some(stage) => stage.heard(news.clone()),
                None => continue,
            };
            asked.extend(self.wanted(asker, wants));
        }
        asked
    }
}
