//! The run itself: it is born, it parks, it wakes, it closes, and it
//! asks to be run wider than the mode it started in.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::events::session::payloads::TokenUsage;
use crate::events::Evidence;
use crate::hash::{CommitSha, ContentHash};
use crate::ids::{ModeName, NodeId, RunId};
use crate::workflow::OnInterrupt;

use super::pause::PauseReason;

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
    /// The git tree the suite was measured on: what a later invocation
    /// takes the measurement's answer for. Absent for a measurement the
    /// run was born holding — it speaks for the tree its measuring run
    /// opened on — and on a log written before the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<crate::TreeId>,
    /// How long the measurement took. Absent where `tree` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
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
    /// The checkout of its own the run works in. Absent for a run working
    /// in a person's checkout, and from a log written before it was
    /// recorded, where the run's checkout is the one named after it under
    /// the worktrees root its manifest froze.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout: Option<std::path::PathBuf>,
    pub inputs: BTreeMap<String, serde_json::Value>,
    pub mode: ModeName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_from: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<crate::SchemaRange>,
    pub base_branch: String,
    pub base_commit: CommitSha,
    /// What the run's commands ran with when it was born. Absent from a
    /// log written before it was recorded. Boxed: every stored event is
    /// as large as the largest payload, and this one is read once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Box<ExecutionEnvironment>>,
    /// The optional nodes this run leaves out because its config lacks
    /// what they need, and the nodes only they lead to. Decided once,
    /// here, so replay never re-reads a config to know its own graph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left_out: Vec<crate::LeftOut>,
    /// Whether the run's tree, when it was born in a checkout of its own,
    /// held exactly the commit it opened on — what lets the suite be
    /// measured in another checkout of that commit while the run goes
    /// on. Absent — false — for a run working in a person's checkout, one
    /// whose tree held more, and a log written before the field: those
    /// measure in their own tree, before anything changes it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub opens_on_base: bool,
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

/// The run moved, between two of its steps, into another checkout of its
/// project's pool that already built the commit the run stands at — as a
/// rule, the one its last unit worked in — and works there from here on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunMovedPayload {
    pub checkout: std::path::PathBuf,
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
    /// The checkout the run works in from this wake on, when it is not the
    /// one it worked in before: the one it last held was given back while
    /// it was parked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout: Option<std::path::PathBuf>,
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
            checkout: None,
        }
    }

    /// The same wake, in `checkout` from now on.
    pub fn in_checkout(self, checkout: std::path::PathBuf) -> Self {
        RunResumedPayload {
            checkout: Some(checkout),
            ..self
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
    /// The person who closed a run nobody was going to continue. Absent
    /// for a run the engine closed by driving it to its end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_by: Option<crate::Responder>,
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
            closed_by: None,
        }
    }

    /// The same close, made by `by` rather than by the engine.
    pub fn closed_by(self, by: crate::Responder) -> Self {
        RunFinishedPayload {
            closed_by: Some(by),
            ..self
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cptv: Option<f64>,
    pub tokens: TokenUsage,
}
