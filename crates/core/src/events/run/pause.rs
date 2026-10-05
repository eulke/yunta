//! Why a run is parked, as a fact, and the one line every surface says
//! for it.

use crate::events::gates::payloads::Escalation;
use crate::events::Failure;
use crate::ids::{NodeId, QuestionId};

/// Why a run is parked, as a fact rather than a sentence.
///
/// Ten call sites used to compose the line a reader sees, each in its own
/// words, and the one line every surface shows for a stopped run
/// therefore said ten different kinds of thing. The fact is stated here
/// and the prose is produced once, by `Display`, at the border.
#[derive(Debug, Clone, PartialEq)]
pub enum PauseReason {
    /// A decision is waiting on a person, with the escalation that asks
    /// it.
    Escalation(Box<Escalation>),
    /// A person stopped the run while it was running.
    Cancelled,
    /// A person stopped a run whose engine was already dead: the
    /// orphaned process groups were killed from outside and the pause
    /// written on the engine's behalf, so the log says who ended the
    /// run and that nothing of it was still alive to ask.
    CancelledAfterCrash,
    /// The run reached `limits.max_tokens_per_run`.
    BudgetExhausted { spent: u64, cap: u64 },
    /// The disk has `available` bytes free, and going on would take
    /// `need` more where `keep_free` must stay free.
    NoRoom {
        available: u64,
        need: u64,
        keep_free: u64,
    },
    /// A loop reached `limits.max_loop_iterations` with tasks still
    /// ready.
    LoopOverrun { node: NodeId, cap: u32 },
    /// A gate is published and the forge has not answered it.
    ExternalGate { url: String },
    /// A resume found nodes still running whose fate it cannot tell.
    UncertainOrphans(Vec<NodeId>),
    /// A node failed and nothing re-routed it.
    NodeFailed { node: NodeId, failure: Failure },
    /// Nothing is runnable: `node` waits on `on`, which this pass left
    /// unresolved.
    Blocked { node: NodeId, on: Vec<NodeId> },
    /// A gate was resolved to abort, with whatever the person wrote.
    GateAborted {
        node: NodeId,
        free_text: Option<String>,
    },
    /// A run this node bore is parked, with the reason that run stated.
    /// The child's own line is carried through rather than restated: the
    /// parent has nothing to add to it, and two sentences about one
    /// pause is one too many.
    ChildPaused { node: NodeId, reason: String },
    /// A node asked questions and no surface could answer them.
    Questions {
        node: NodeId,
        pending: crate::NonEmpty<QuestionId>,
    },
    /// A surface answered and the questions refused the reply, with
    /// every way it failed to answer them.
    AnswersRefused {
        node: NodeId,
        report: crate::diagnostic::Report,
    },
}

/// The half every cap's sentence ends with: a reader who hit a declared
/// limit has the same two ways past it whichever limit it was.
const PAST_THE_CAP: &str = "resume with an interactive surface to continue past the cap or abort";

impl std::fmt::Display for PauseReason {
    /// The one line a reader sees for a parked run. Every surface — the
    /// listing's row, the status heading, the line a resume echoes —
    /// prints this, so none of them can say something different.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PauseReason::Escalation(escalation) => write!(f, "{}", escalation.sentence()),
            PauseReason::Cancelled => f.write_str("cancelled by user"),
            PauseReason::CancelledAfterCrash => f.write_str("cancelled after crash"),
            PauseReason::BudgetExhausted { spent, cap } => write!(
                f,
                "budget: run spent {} with `limits.max_tokens_per_run: {cap}` — {PAST_THE_CAP}",
                crate::units::Tokens(*spent).exact()
            ),
            PauseReason::NoRoom {
                available,
                need,
                keep_free,
            } => f.write_str(&no_room(*available, *need, *keep_free)),
            PauseReason::LoopOverrun { node, cap } => write!(
                f,
                "loop `{node}` exceeded `limits.max_loop_iterations` ({cap}) — {PAST_THE_CAP}"
            ),
            PauseReason::ExternalGate { url } => write!(f, "waiting on external gate: {url}"),
            PauseReason::UncertainOrphans(nodes) => f.write_str(&uncertain(nodes)),
            // The claim; what a command printed travels as the
            // escalation's evidence, not inside the one line a pause is.
            PauseReason::NodeFailed { node, failure } => {
                write!(f, "node `{node}` failed: {}", failure.headline())
            }
            PauseReason::Blocked { node, on } => write!(
                f,
                "no node is runnable: `{node}` waits on {}, which this run left unresolved",
                crate::text::listed(on.iter().map(NodeId::as_str))
            ),
            PauseReason::GateAborted { node, free_text } => f.write_str(&crate::text::detailed(
                format!("node `{node}`'s gate was resolved to abort"),
                free_text.as_deref().unwrap_or_default(),
            )),
            PauseReason::ChildPaused { node, reason } => {
                write!(f, "node `{node}`'s child run is parked: {reason}")
            }
            PauseReason::Questions { node, pending } => write!(
                f,
                "node `{node}` {}",
                crate::text::asked_questions(pending.as_slice())
            ),
            PauseReason::AnswersRefused { node, report } => {
                write!(f, "node `{node}`'s answers were refused: {report}")
            }
        }
    }
}

/// Why a run stops for want of room on the disk, and how to make some.
fn no_room(available: u64, need: u64, keep_free: u64) -> String {
    use crate::units::Bytes;
    let building = match need {
        0 => String::new(),
        need => format!(", and a build here takes about {} more", Bytes(need)),
    };
    format!(
        "no room: the disk has {} free{building}, where {} stays free — make room \
         (`yunta gc`, close a parked run, or lower `storage.pool.keep_free`) and resume",
        Bytes(available),
        Bytes(keep_free)
    )
}

/// Why a run stops when it cannot tell how `nodes` ended.
fn uncertain(nodes: &[NodeId]) -> String {
    format!(
        "{} running with no terminal event when the engine last stopped: {} — \
         `on_interrupt: fail_if_uncertain` refuses to guess how each ended; verify manually \
         before resuming",
        crate::text::agreeing(nodes.len(), "a node was", "nodes were"),
        crate::text::listed(nodes.iter().map(NodeId::as_str))
    )
}
