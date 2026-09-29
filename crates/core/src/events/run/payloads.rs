//! The run itself: it is born, it parks, it wakes, it closes, and it
//! asks to be run wider than the mode it started in.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::events::gates::payloads::Escalation;
use crate::events::session::payloads::TokenUsage;
use crate::events::{Evidence, Failure};
use crate::hash::{CommitSha, ContentHash};
use crate::ids::{ModeName, NodeId, QuestionId, RunId};
use crate::workflow::OnInterrupt;

/// Exact variant names are provisional; a `cancel` command
/// exists, so `Cancelled` is included alongside the obvious two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Done,
    Failed,
    Cancelled,
    /// The run closed because its own gate accepted promotion
    /// to a later-declared mode — never because the work itself failed
    /// or was cancelled.
    Promoted,
}

// --- Per-kind payloads ------------------------------------------------

/// What the suite a run's lineage declared did on the tree the run
/// opens on, and whose measurement it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BaselineCapturedPayload {
    pub command: String,
    pub results: BaselineResults,
    pub hash: ContentHash,
    /// Whose measurement this is. A log written before the field reads
    /// [`BaselineOrigin::Measured`], which is what such a log meant.
    #[serde(default)]
    pub origin: BaselineOrigin,
}

impl BaselineCapturedPayload {
    /// Whether the suite passed when it was measured — the only
    /// measurement against which something can be seen to stop passing.
    pub fn passed(&self) -> bool {
        self.results.exit_code == 0
    }
}

/// What the suite reported: the code it exited with, and the tail a
/// reader sees without opening what the measuring run kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BaselineResults {
    pub exit_code: i32,
    pub summary: String,
}

/// Who took the measurement a run holds: this run, or the root of the
/// lineage it was born into.
///
/// A lineage measures once. A run born of another — a `kind: workflow`
/// child, a promotion successor — is born holding the root's
/// measurement, so every comparison anywhere in the lineage answers the
/// same question: what worked before the invocation started.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BaselineOrigin {
    /// This run ran the suite itself, on its first wake.
    #[default]
    Measured,
    /// The run was born holding it; `run` is the root that measured,
    /// never the parent it was handed down through.
    Inherited { run: RunId },
}

/// The host the run works on was suspended — the machine slept — for
/// `slept_ms`. The event's timestamp is when the engine noticed, at or
/// just after the host woke, so the suspension spans the `slept_ms`
/// before it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HostSuspendedPayload {
    pub slept_ms: u64,
}

impl HostSuspendedPayload {
    /// A suspension that lasted `slept`.
    pub fn slept(slept: std::time::Duration) -> Self {
        HostSuspendedPayload {
            slept_ms: u64::try_from(slept.as_millis()).unwrap_or(u64::MAX),
        }
    }

    /// How long the host slept.
    pub fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.slept_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunCreatedPayload {
    pub manifest_hash: ContentHash,
    pub inputs: BTreeMap<String, serde_json::Value>,
    pub mode: ModeName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_from: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<crate::SchemaRange>,
    pub base_branch: String,
    pub base_commit: CommitSha,
    /// What the run's commands ran with when it was born. Absent from a
    /// log written before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<ExecutionEnvironment>,
}

/// What the engine hands every command it runs — criteria, `bash` nodes,
/// hooks: the shell that interprets it, and the directories that shell
/// looks programs up in, in order. A command that works in an agent's
/// own shell proves nothing about this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExecutionEnvironment {
    pub shell: String,
    pub path: Vec<String>,
}

impl ExecutionEnvironment {
    /// What changed from this environment to `now`, or `None` when
    /// nothing a command could notice did.
    pub fn drift_to(&self, now: &ExecutionEnvironment) -> Option<EnvironmentDrift> {
        let gained: Vec<String> = now
            .path
            .iter()
            .filter(|dir| !self.path.contains(dir))
            .cloned()
            .collect();
        let lost: Vec<String> = self
            .path
            .iter()
            .filter(|dir| !now.path.contains(dir))
            .cloned()
            .collect();
        let shell = (self.shell != now.shell).then(|| (self.shell.clone(), now.shell.clone()));
        let reordered = gained.is_empty() && lost.is_empty() && self.path != now.path;
        (shell.is_some() || reordered || !gained.is_empty() || !lost.is_empty()).then_some(
            EnvironmentDrift {
                shell,
                gained,
                lost,
                reordered,
            },
        )
    }
}

impl std::fmt::Display for ExecutionEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} with PATH={}", self.shell, self.path.join(":"))
    }
}

/// How the environment a run's commands run with changed between the
/// run's birth and its latest wake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentDrift {
    /// The shell before and after, when it changed.
    pub shell: Option<(String, String)>,
    /// `PATH` directories the latest wake has and the birth did not.
    pub gained: Vec<String>,
    /// `PATH` directories the birth had and the latest wake does not.
    pub lost: Vec<String>,
    /// The same directories, looked up in another order.
    pub reordered: bool,
}

impl std::fmt::Display for EnvironmentDrift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut said: Vec<String> = Vec::new();
        if let Some((before, after)) = &self.shell {
            said.push(format!("the shell went from {before} to {after}"));
        }
        if !self.gained.is_empty() {
            said.push(format!("PATH gained {}", self.gained.join(", ")));
        }
        if !self.lost.is_empty() {
            said.push(format!("PATH lost {}", self.lost.join(", ")));
        }
        if self.reordered {
            said.push("PATH looks its directories up in another order".to_string());
        }
        write!(
            f,
            "commands now run in another environment than the run was born in: {}",
            said.join("; ")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PromotionSignaledPayload {
    /// Why the run promoted, on one line — the claim and the facts
    /// behind it, since this is the only field a reader of the event
    /// itself gets.
    pub reason: String,
    /// The record `reason` is built from, kept apart so a surface can
    /// show it under its own heading.
    pub evidence: Evidence,
    pub suggested_mode: ModeName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunPausedPayload {
    reason: String,
}

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
                "budget: run spent {spent} tokens with `limits.max_tokens_per_run: {cap}` — \
                 {PAST_THE_CAP}"
            ),
            PauseReason::LoopOverrun { node, cap } => write!(
                f,
                "loop `{node}` exceeded `limits.max_loop_iterations` ({cap}) — {PAST_THE_CAP}"
            ),
            PauseReason::ExternalGate { url } => write!(f, "waiting on external gate: {url}"),
            PauseReason::UncertainOrphans(nodes) => write!(
                f,
                "node(s) {} were running with no terminal event when the engine last \
                 stopped — `on_interrupt: fail_if_uncertain` refuses to guess whether they \
                 finished; verify manually before resuming",
                crate::text::listed(nodes.iter().map(NodeId::as_str))
            ),
            PauseReason::NodeFailed { node, failure } => {
                write!(f, "node `{node}` failed: {failure}")
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

impl RunPausedPayload {
    /// A run parked, with the one line a reader gets for why.
    ///
    /// The reason is what every surface prints for a stopped run — the
    /// listing's row, the status page's heading, the line a resume
    /// echoes — so it is composed once, here, rather than invented at
    /// each place that decides to stop.
    pub fn new(reason: &PauseReason) -> Self {
        RunPausedPayload {
            reason: reason.to_string(),
        }
    }

    /// A pause read back off a log, whose reason is already the line a
    /// writer produced. The only way to build one from prose, and it is
    /// for reading: a run that parks states the fact.
    pub fn recorded(reason: impl Into<String>) -> Self {
        RunPausedPayload {
            reason: reason.into(),
        }
    }

    /// Why the run is parked.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunResumedPayload {
    /// The one `on_interrupt` every orphan of this resume resolved to;
    /// absent when the resume found no orphan or their policies differ
    /// — `policies` is the record either way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_policy_applied: Option<String>,
    /// Every node the log left running with no terminal event, and the
    /// `on_interrupt` it resolved to: its own, or the config's default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<ResumePolicy>,
    /// What the run's commands run with from this wake on. Absent from a
    /// log written before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<ExecutionEnvironment>,
}

impl RunResumedPayload {
    /// A run woken, with what it found still running and what each of
    /// those resolved to.
    ///
    /// `resume_policy_applied` is derived, never passed: it is the one
    /// policy every orphan agreed on, and there is no such policy when
    /// the resume found no orphan or when two of them resolved
    /// differently. Deriving it here is what keeps the summary and the
    /// record from disagreeing.
    pub fn new(policies: Vec<ResumePolicy>, environment: Option<ExecutionEnvironment>) -> Self {
        let agreed = policies.split_first().and_then(|(first, rest)| {
            rest.iter()
                .all(|policy| policy.on_interrupt == first.on_interrupt)
                .then(|| first.on_interrupt.as_str().to_string())
        });
        RunResumedPayload {
            resume_policy_applied: agreed,
            policies,
            environment,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResumePolicy {
    pub node: NodeId,
    pub on_interrupt: OnInterrupt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunFinishedPayload {
    pub terminal_state: TerminalState,
    pub metrics: RunMetrics,
}

impl RunFinishedPayload {
    /// A run closed at `terminal`, with the metrics its log implies.
    ///
    /// The metrics are derived here and nowhere else: cost per verified
    /// task is total spend over the tasks that actually reached `done`,
    /// and a run that verified none has no such cost — `None`, never a
    /// zero that reads like a free run. Each of the three ways a run
    /// closes passes the same two numbers and gets the same arithmetic.
    pub fn closed(terminal: TerminalState, tokens: TokenUsage, tasks_done: usize) -> Self {
        RunFinishedPayload {
            terminal_state: terminal,
            metrics: RunMetrics {
                cptv: (tasks_done > 0).then(|| tokens.total() as f64 / tasks_done as f64),
                tokens,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cptv: Option<f64>,
    pub tokens: TokenUsage,
}
