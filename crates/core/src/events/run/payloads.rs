//! The run itself: it is born, it parks, it wakes, it closes, and it
//! asks to be run wider than the mode it started in.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::events::Evidence;
use crate::hash::{CommitSha, ContentHash};
use crate::ids::{ModeName, NodeId, RunId};
use crate::workflow::OnInterrupt;

use super::metrics::RunMetrics;
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

/// What one project command `baseline.held` names did on the tree the
/// run's lineage measured its suite on, and whose measurement it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BaselineHeldPayload {
    /// The command's name, as `commands:` declares it.
    pub name: crate::ids::CommandName,
    /// What the project runs for it.
    pub command: String,
    pub results: BaselineResults,
    pub origin: BaselineOrigin,
    /// The git tree it was measured on; absent for a measurement the run
    /// was born holding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<crate::TreeId>,
    /// How long it took. Absent where `tree` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

impl BaselineHeldPayload {
    /// Whether it passed when it was measured — whether it holds the
    /// run's tasks at all.
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

/// Where a run's promotion leads: the node that exhausted its re-routes,
/// and the node it was routed to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PromotionCause {
    pub node: NodeId,
    pub goto: NodeId,
}

/// A run asked to be run wider than the mode it started in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(transform = without_legacy_reason)]
pub struct PromotionSignaledPayload {
    /// What asked for the promotion. Absent from a log written before the
    /// cause was recorded, which carries only the sentence `reason` holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<PromotionCause>,
    /// The record the promotion rests on, kept apart so a surface can show
    /// it under its own heading.
    pub evidence: Evidence,
    pub suggested_mode: ModeName,
    /// The sentence an older writer stored in place of `cause`. Read from
    /// such a log, never written.
    #[serde(default, skip_serializing)]
    reason: Option<String>,
}

impl PromotionSignaledPayload {
    /// A promotion the run asks for, with the cause that asks it.
    pub fn new(cause: PromotionCause, evidence: Evidence, suggested_mode: ModeName) -> Self {
        PromotionSignaledPayload {
            cause: Some(cause),
            evidence,
            suggested_mode,
            reason: None,
        }
    }

    /// The sentence an older log stored in place of the cause, as written.
    pub fn legacy_reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

/// Why the run is parked: the pause the engine recorded, as a fact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "RunPausedWire", into = "RunPausedLine")]
// The published shape is the one a writer writes. The read shape would
// publish an optional pause and the sentence an older writer stored.
#[schemars(with = "RunPausedLine")]
pub struct RunPausedPayload {
    pub pause: PauseReason,
}

/// What a `run_paused` line may hold on reading: the pause, or the sentence
/// an older writer stored in its place. Only reading produces `reason`.
#[derive(Deserialize)]
struct RunPausedWire {
    #[serde(default)]
    pause: Option<PauseReason>,
    #[serde(default)]
    reason: Option<String>,
}

/// What a writer writes: the pause, which every line carries. The published
/// schema is this shape, so `pause` is required and no sentence appears.
#[derive(Serialize, schemars::JsonSchema)]
struct RunPausedLine {
    pause: PauseReason,
}

impl TryFrom<RunPausedWire> for RunPausedPayload {
    type Error = String;

    /// A line with neither field names no pause, so it does not fit its
    /// kind and a read refuses it.
    fn try_from(wire: RunPausedWire) -> Result<Self, Self::Error> {
        let pause = match (wire.pause, wire.reason) {
            (Some(pause), _) => pause,
            (None, Some(sentence)) => PauseReason::Recorded { sentence },
            (None, None) => return Err("a run_paused names its pause or its reason".to_string()),
        };
        Ok(RunPausedPayload { pause })
    }
}

impl From<RunPausedPayload> for RunPausedLine {
    fn from(payload: RunPausedPayload) -> Self {
        RunPausedLine {
            pause: payload.pause,
        }
    }
}

impl RunPausedPayload {
    /// A run parked for `pause`.
    pub fn new(pause: &PauseReason) -> Self {
        RunPausedPayload {
            pause: pause.clone(),
        }
    }
}

/// Keeps a legacy field out of a published shape: a writer never writes
/// it, so the schema shows only what a writer writes.
fn without_legacy_reason(schema: &mut schemars::Schema) {
    if let Some(serde_json::Value::Object(properties)) = schema.get_mut("properties") {
        properties.remove("reason");
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
    pub fn new(policies: Vec<ResumePolicy>, environment: Option<ExecutionEnvironment>) -> Self {
        RunResumedPayload {
            policies,
            environment,
            checkout: None,
        }
    }

    /// The one `on_interrupt` every orphan of this wake resolved to. There
    /// is no such policy when the wake found no orphan, or when two of
    /// them resolved differently; `policies` is the record either way.
    pub fn agreed_policy(&self) -> Option<String> {
        let (first, rest) = self.policies.split_first()?;
        rest.iter()
            .all(|policy| policy.on_interrupt == first.on_interrupt)
            .then(|| first.on_interrupt.as_str().to_string())
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
    /// A run closed at `terminal`, recording the metrics it was given.
    pub fn closed(terminal: TerminalState, metrics: RunMetrics) -> Self {
        RunFinishedPayload {
            terminal_state: terminal,
            metrics,
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

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::schema_for;

    fn orphan(node: &'static str, on_interrupt: OnInterrupt) -> ResumePolicy {
        ResumePolicy {
            node: NodeId::from_static(node),
            on_interrupt,
        }
    }

    /// Orphans that resolved alike agree on their policy; orphans that
    /// resolved differently, or none at all, agree on none.
    #[test]
    fn orphans_agree_on_a_policy_only_when_they_resolved_alike() {
        let (restart, fail) = (OnInterrupt::RestartNode, OnInterrupt::FailIfUncertain);
        let alike = RunResumedPayload::new(vec![orphan("a", restart), orphan("b", restart)], None);
        let unlike = RunResumedPayload::new(vec![orphan("a", restart), orphan("b", fail)], None);
        let empty = RunResumedPayload::new(Vec::new(), None);
        assert_eq!(alike.agreed_policy().as_deref(), Some("restart_node"));
        assert_eq!(unlike.agreed_policy(), None);
        assert_eq!(empty.agreed_policy(), None);
    }

    /// A `run_resumed` an older writer stamped with the policy it agreed on
    /// reads back, with that field ignored.
    #[test]
    fn a_legacy_resume_policy_field_is_ignored() {
        let legacy = serde_json::json!({
            "resume_policy_applied": "restart_node",
            "policies": [{ "node": "build", "on_interrupt": "restart_node" }],
        });
        let read: RunResumedPayload = serde_json::from_value(legacy).unwrap();
        assert_eq!(read.agreed_policy().as_deref(), Some("restart_node"));
    }

    /// The published shapes carry the pause and the cause, and never the
    /// legacy sentences a log may still hold.
    #[test]
    fn the_published_shapes_leave_the_legacy_sentences_out() {
        let run = serde_json::to_value(schema_for!(RunPausedPayload)).unwrap();
        let promotion = serde_json::to_value(schema_for!(PromotionSignaledPayload)).unwrap();
        let (run, promotion) = (&run["properties"], &promotion["properties"]);
        assert!(run.get("pause").is_some(), "{run}");
        assert!(run.get("reason").is_none(), "{run}");
        assert!(promotion.get("cause").is_some(), "{promotion}");
        assert!(promotion.get("reason").is_none(), "{promotion}");
    }
}
