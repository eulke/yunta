//! See [`crate`]. Answering a run that waits on a person: the decision its
//! gate waits on, read with what it shows, or the round of questions one of
//! its nodes asked, each read by one function of the run's manifest, directory
//! and state that a run opened and a run followed both read through; and a
//! decision recorded on its log through the engine's one consequence path,
//! after which a detached resume applies it. A run another engine drives is
//! refused before anything is written; a run whose decision is on its log but
//! that could not be handed on says so as recorded, because a reader who took
//! it for a refusal would answer the same gate twice.

use std::collections::BTreeMap;
use std::path::Path;

use yunta_core::ahead::NodesAhead;
use yunta_core::diagnostic::Report;
use yunta_core::events::{
    ChangesCountedPayload, DecisionOn, GateWaitingPayload, HumanChoice, PauseReason, StoredEvent,
};
use yunta_core::port::QuestionsReply;
use yunta_core::refusal::AnswersRefusal;
use yunta_core::shown::ShownDocument;
use yunta_core::{Manifest, NodeId, OptionId, QuestionsFile, Responder, RunId};
use yunta_render::escalation::Route;
use yunta_render::refusal::Refused;
use yunta_replay::frame::{RunFrame, WaitingOn};
use yunta_replay::RunState;

use crate::{Error, Failure, OpenedRun};

/// What a run waits on a person for: the decision its gate waits on, with what it shows,
/// or the round of questions one of its nodes asked. Not
/// [`WaitingOn`], where a run is parked.
#[derive(Debug, Clone)]
pub enum Awaited {
    Decision(Box<WaitingDecision>),
    Round(Box<WaitingQuestions>),
}

/// The decision a run waits on, with the documents it shows and what the menu a person
/// picks from is drawn from.
#[derive(Debug, Clone)]
pub struct WaitingDecision {
    pub on: DecisionOn,
    pub escalation: GateWaitingPayload,
    pub shown: Vec<ShownDocument>,
    /// What follows each option that leads somewhere.
    pub routes: BTreeMap<OptionId, Route>,
    /// What the run changed, when the change was counted on its newest tree.
    pub changed: Option<ChangesCountedPayload>,
    /// The workflow the run follows, by its name.
    pub workflow: String,
    /// What lies ahead at the nodes its options lead to, as the run froze them.
    pub ahead: NodesAhead,
}

/// The questions a node parked on and nobody has answered: the node, the round it asked,
/// as the document it handed over holds it, and what the engine refused of the answers the
/// round was last given.
#[derive(Debug, Clone)]
pub struct WaitingQuestions {
    pub node: NodeId,
    pub questions: QuestionsFile,
    /// Every way the answers the round was last given failed it, as the run's own pause
    /// records them, where the run stands parked on that refusal: none for a round asked and
    /// not answered yet.
    pub refused: Option<Report>,
}

/// The node whose round of questions the run framed as `frame` is parked on, asked and
/// not answered, or answered with answers the engine refused; `None` for a run parked
/// on anything else, or not parked.
pub fn waiting_round(frame: &RunFrame) -> Option<&NodeId> {
    match frame.phase.waiting_on().and_then(|on| on.reason()) {
        Some(PauseReason::Questions { node, .. } | PauseReason::AnswersRefused { node, .. }) => {
            Some(node)
        }
        _ => None,
    }
}

/// What the engine refused of the answers `node` last gave its round, as the run's own pause
/// `paused` records it: none where the run is not parked on that refusal.
fn refused_of(paused: Option<&PauseReason>, node: &NodeId) -> Option<Report> {
    match paused? {
        PauseReason::AnswersRefused {
            node: refused,
            report,
        } if refused == node => Some(report.clone()),
        _ => None,
    }
}

impl WaitingDecision {
    /// The decision the run whose manifest is `manifest`, whose directory is `dir` and
    /// whose events derive `state` waits on, with the documents it shows: none when it
    /// waits on none, as a decision answered elsewhere reads.
    pub(crate) async fn read(
        manifest: &Manifest,
        dir: &Path,
        state: &RunState,
    ) -> Result<Option<WaitingDecision>, Error> {
        let decision = yunta_engine::current_escalation(manifest, dir, state).await?;
        let Some((on, escalation)) = decision else {
            return Ok(None);
        };
        let escalation = escalation.into_payload();
        let asked = manifest.request();
        let shown = yunta_engine::shown_documents(dir, escalation.shows(), state, asked).await?;
        let workflow = &manifest.workflow;
        let routes = yunta_render::escalation::routes(
            &escalation,
            workflow,
            state.run.left_out(),
            on.node(),
        );
        let ahead = NodesAhead::of(workflow, &manifest.config, state.run.left_out());
        Ok(Some(WaitingDecision {
            routes,
            ahead,
            changed: state.current_change().cloned(),
            workflow: workflow.name.to_string(),
            on,
            escalation,
            shown,
        }))
    }
}

impl WaitingQuestions {
    /// The round of questions `node` asked and nobody has answered, read from the
    /// document it handed over in the run whose directory is `dir`, by the hash its
    /// `questions_asked` in `events` recorded, with what the run's own pause, `paused`,
    /// records the engine refused of the answers it was last given: none when it waits on
    /// no round, as a round answered elsewhere reads.
    pub(crate) async fn read(
        dir: &Path,
        events: &[StoredEvent],
        node: NodeId,
        paused: Option<&PauseReason>,
    ) -> Result<Option<WaitingQuestions>, Error> {
        let refused = refused_of(paused, &node);
        match yunta_engine::asked_questions(dir, events, &node).await {
            Ok(questions) => Ok(Some(WaitingQuestions {
                node,
                questions,
                refused,
            })),
            Err(yunta_engine::AnswerQuestionsError::Refused(AnswersRefusal::NothingAsked {
                ..
            })) => Ok(None),
            Err(yunta_engine::AnswerQuestionsError::Refused(refusal)) => {
                Err(Refused::Answers(refusal).into())
            }
            Err(failed) => Err(Failure::new(failed).into()),
        }
    }
}

impl OpenedRun<'_> {
    /// The questions this run is parked on, read from the document the node handed
    /// over, by the hash its `questions_asked` recorded: refused when it waits on
    /// none, as a run that waits on no decision is.
    pub async fn waiting_questions(&self) -> Result<WaitingQuestions, Error> {
        let frame = self.frame();
        let Some(node) = waiting_round(&frame).cloned() else {
            return Err(Refused::NoDecision {
                run: self.id.clone(),
            }
            .into());
        };
        let nothing_asked = Refused::Answers(AnswersRefusal::NothingAsked { node: node.clone() });
        let paused = frame.phase.waiting_on().and_then(WaitingOn::reason);
        WaitingQuestions::read(&self.dir, &self.events, node, paused)
            .await?
            .ok_or_else(|| nothing_asked.into())
    }

    /// The decision this run waits on, with what it shows: refused when it
    /// waits on none, naming the command that says where it stands.
    pub async fn waiting_decision(&self) -> Result<WaitingDecision, Error> {
        let state = RunState::replay(&self.events);
        let no_decision = Refused::NoDecision {
            run: self.id.clone(),
        };
        WaitingDecision::read(&self.manifest.doc, &self.dir, &state)
            .await?
            .ok_or_else(|| no_decision.into())
    }

    /// Records `option`, with the words a person added to it, on the
    /// decision this run waits on, then hands the run to a detached resume
    /// that applies it. Who decided is `by`, or the workspace's user marked
    /// unverified. Refused before anything is written when another engine
    /// drives the run or the decision does not take `option`.
    pub async fn decide(
        &self,
        option: OptionId,
        by: Option<&Responder>,
        free_text: Option<String>,
    ) -> Result<(), Error> {
        // Said here because the resume would say it only to its own log, with
        // the answer already recorded and the run reading as moving.
        self.refuse_if_driven().await?;
        let project = self.project;
        let workspace = &project.workspace;
        let manifest = &self.manifest.doc;
        let storage = project.storage().await?;
        let redactor = yunta_engine::run_redactor(manifest, workspace.secrets.as_ref())?;
        let choice = HumanChoice {
            option,
            by: Responder::answering(by, workspace.env.user.as_deref()),
            free_text,
        };
        let clock = workspace.clock.as_ref();
        yunta_engine::resolve_gate(
            manifest, &self.dir, &storage, &self.id, clock, choice, &redactor,
        )
        .await
        .map_err(|error| unrecorded(&self.id, error))?;
        self.handed_on(Recorded::Decision).await
    }

    /// Records `reply` to the questions node `node` asked, then hands the run
    /// on to a detached resume that carries the node on, as after a gate is
    /// resolved. The round is read again from the document the node asked
    /// from and the reply judged against it, and either both the acceptance
    /// and `questions_answered` land or neither does: a reply that does not
    /// answer is refused, every way it fails named. Who answered is the
    /// reply's claim, or the workspace's user marked unverified.
    pub async fn answer(&self, node: &NodeId, reply: QuestionsReply) -> Result<(), Error> {
        // Said here because the resume would say it only to its own log, with
        // the answers already recorded.
        self.refuse_if_driven().await?;
        let project = self.project;
        let workspace = &project.workspace;
        let manifest = &self.manifest.doc;
        let storage = project.storage().await?;
        let redactor = yunta_engine::run_redactor(manifest, workspace.secrets.as_ref())?;
        let responder =
            Responder::answering(reply.responder.as_ref(), workspace.env.user.as_deref());
        let reply = yunta_engine::AnswersReply {
            answers: reply.answers,
            channel: reply.channel,
            responder: Some(responder),
        };
        let clock = workspace.clock.as_ref();
        yunta_engine::answer_questions(
            &redactor, &storage, &self.id, &self.dir, clock, node, reply,
        )
        .await
        .map_err(|error| match error {
            yunta_engine::AnswerQuestionsError::Refused(refusal) => {
                Error::from(Refused::Answers(refusal))
            }
            failed => Failure::new(failed).into(),
        })?;
        self.handed_on(Recorded::Answers).await
    }

    /// Hands the run on to a detached resume once what `recorded` names is on
    /// its log; a hand-on that fails says that was recorded all the same.
    pub(crate) async fn handed_on(&self, recorded: Recorded) -> Result<(), Error> {
        match self.resume_detached().await {
            Ok(_) => Ok(()),
            Err(Error::Failed(failure)) => {
                Err(Failure::new(RecordedNotResumed { recorded, failure }).into())
            }
            Err(other) => Err(other),
        }
    }
}

/// What a decision run `id` would not take, or could not record, is.
pub(crate) fn unrecorded(id: &RunId, error: yunta_engine::ResolveGateError) -> Error {
    match error {
        yunta_engine::ResolveGateError::Refused(refusal) => Refused::Gate {
            run: id.clone(),
            refusal,
        }
        .into(),
        yunta_engine::ResolveGateError::Storage(error) => Failure::new(error).into(),
        yunta_engine::ResolveGateError::Documents(run) => {
            Failure::new(GateDocumentsUnread(run)).into()
        }
    }
}

/// What was recorded on a run's log before it was handed on.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Recorded {
    Decision,
    Answers,
}

/// A decision or answers on the log of a run that could not be handed back
/// to a detached resume. The sentence leads with what did happen, because a
/// reader who takes this for a refusal answers the same gate twice.
#[derive(Debug)]
struct RecordedNotResumed {
    recorded: Recorded,
    failure: Failure,
}

impl std::fmt::Display for RecordedNotResumed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let recorded = match self.recorded {
            Recorded::Decision => "decision",
            Recorded::Answers => "answers",
        };
        write!(f, "{recorded} recorded, but {}", self.failure)
    }
}

/// The causes are the hand-on's own, as the failure behind it holds them.
impl std::error::Error for RecordedNotResumed {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.failure)
    }
}

/// The documents a decision shows that could not be read, so its menu cannot
/// be rebuilt as a person would be shown it: the run's error that says why,
/// with the causes behind it.
#[derive(Debug)]
struct GateDocumentsUnread(Box<yunta_engine::RunError>);

impl std::fmt::Display for GateDocumentsUnread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let why = match self.0.as_ref() {
            yunta_engine::RunError::Object(problem) => yunta_render::failure::object(problem),
            other => other.to_string(),
        };
        write!(f, "the documents the decision shows cannot be read: {why}")
    }
}

/// The causes are the run error's own, past the error the sentence says.
impl std::error::Error for GateDocumentsUnread {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(self.0.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::ObjectProblem;
    use yunta_core::sha256_hex;

    use super::GateDocumentsUnread;
    use crate::Failure;

    /// A decision whose documents cannot be read names the object they lack
    /// once: in the object's words, with no cause saying it again.
    #[test]
    fn a_decision_whose_documents_cannot_be_read_names_the_object_once() {
        let run = yunta_engine::RunError::Object(ObjectProblem::Missing {
            hash: sha256_hex(b"plan"),
        });
        let said = Failure::new(GateDocumentsUnread(Box::new(run))).said();
        assert!(
            said.starts_with(
                "the documents the decision shows cannot be read: the run holds no object `"
            ),
            "{said}"
        );
        assert_eq!(said.matches("the run holds no object").count(), 1, "{said}");
    }
}
