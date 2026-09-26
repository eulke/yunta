//! The words themselves, one function per event domain.
//!
//! Each domain reads its own kinds and nothing else, so a kind added
//! to one never silently borrows another's phrasing. Every state word
//! comes from `NodeDisplay`, every run or child close from
//! `view::closed_as`, every enum its own published word — nothing here
//! formats a domain type with `Debug`.

use yunta_core::events::{
    artifacts, children, findings, gates, node, run, scope, session, tasks, BaselineOrigin,
    GateResolvedPayload, TaskStatus,
};
use yunta_core::fence::Coverage;
use yunta_core::text::{detailed, one_line};
use yunta_core::NonEmpty;
use yunta_engine::{Happening, NodeState, NodeWait};

use super::super::view;
use crate::render::{format_duration, NodeDisplay, StateWord};
use tasks::happening as tasks_happening;

/// What the moment carries beyond its subject, and the state it is
/// marked with.
pub(super) fn carried(happening: &Happening) -> (Option<StateWord>, String) {
    match happening {
        Happening::Run(it) => (None, run_words(it)),
        Happening::Node(it) => node_words(it),
        Happening::Session(it) => (None, session_words(it)),
        Happening::Tasks(it) => (None, task_words(it)),
        Happening::Scope(it) => (None, scope_words(it)),
        Happening::Findings(it) => (None, finding_words(it)),
        Happening::Artifacts(it) => (None, artifact_words(it)),
        Happening::Gates(it) => gate_words(it),
        Happening::Children(it) => child_words(it),
        Happening::Unknown { kind } => (
            Some(StateWord::Wait),
            format!("`{kind}`, a kind this binary does not read"),
        ),
    }
}

fn run_words(happening: &run::happening::Happening) -> String {
    use run::happening::Happening as H;
    match happening {
        H::Created { mode, base_branch } => format!("created — mode `{mode}` off {base_branch}"),
        H::Paused { reason } => format!("paused — {reason}"),
        H::Resumed { policies } => match policies.len() {
            0 => "resumed".to_string(),
            n => format!(
                "resumed — {} settled",
                yunta_core::text::counted(n, "orphan")
            ),
        },
        H::BaselineCaptured(origin) => match origin {
            BaselineOrigin::Measured => "baseline measured".to_string(),
            BaselineOrigin::Inherited { run } => {
                format!("baseline inherited from run {run}")
            }
        },
        H::Closed { terminal, .. } => view::closed_as(*terminal).to_string(),
        H::PromotionSignaled { to, reason, .. } => {
            detailed(format!("promotion to `{to}`"), &one_line(reason))
        }
    }
}

fn node_words(happening: &node::happening::Happening) -> (Option<StateWord>, String) {
    use node::happening::Happening as H;
    match happening {
        H::RunnerResolved(it) => (
            None,
            format!("{} on {}/{}", it.runner, it.chosen.adapter, it.chosen.model),
        ),
        H::Reached { state, elapsed, .. } => {
            let display = NodeDisplay::of(Some(state));
            let worked = elapsed
                .map(|elapsed| format!(" · {}", format_duration(elapsed)))
                .unwrap_or_default();
            (Some(display.word), format!("{}{worked}", display.label()))
        }
        H::Rerouted(it) => (
            Some(StateWord::Wait),
            detailed(format!("rerouted to `{}`", it.to), &one_line(&it.cause)),
        ),
        H::HookRan { phase, exit_code } => {
            (None, format!("hook {} exit {exit_code}", phase.as_str()))
        }
        H::ContextAssembled => (None, "context assembled".to_string()),
        H::CriteriaChecked {
            task,
            phase,
            checked,
        } => (
            None,
            format!(
                "{task} {}: {}",
                phase.as_str(),
                yunta_core::text::counted(*checked, "criterion")
            ),
        ),
        H::ScopeChecked { violations } => (
            None,
            format!(
                "{} out of scope",
                yunta_core::text::counted(*violations, "path")
            ),
        ),
    }
}

fn session_words(happening: &session::happening::Happening) -> String {
    use session::happening::Happening as H;
    match happening {
        H::Opened {
            agent,
            model,
            fence,
        } => {
            let mut said = "session opened".to_string();
            if let Some(agent) = agent {
                said.push_str(&format!(" as {agent}"));
            }
            if let Some(model) = model {
                said.push_str(&format!(" on {model}"));
            }
            if let Some(coverage) = fence {
                said.push_str(&format!(" · {}", fence_covered(coverage)));
            }
            said
        }
        H::Called { tool, .. } => match tool {
            Some(tool) => format!("called {tool}"),
            None => "called a tool it did not name".to_string(),
        },
        H::Message(kind) => kind.as_str().to_string(),
        H::Degraded {
            capability,
            adapter,
            policy,
        } => detailed(
            format!("{} not declared by {adapter}", capability.as_str()),
            &one_line(policy),
        ),
        H::Refused(target) => format!("write refused: {}", target.sentence()),
        H::RunToolFailed { tool, cause } => {
            format!("run tool call failed: {} ({})", tool.name(), cause.as_str())
        }
    }
}

fn task_words(happening: &tasks_happening::Happening) -> String {
    use tasks_happening::Happening as H;
    match happening {
        H::Registered { task } => format!("task {task} registered"),
        H::Moved { task, to } => format!("{task} is {}", status(*to)),
    }
}

/// A task's status in the words the event schema publishes, never a
/// Rust identifier.
fn status(status: TaskStatus) -> &'static str {
    crate::commands::status::task_status_label(status)
}

fn scope_words(happening: &scope::happening::Happening) -> String {
    use scope::happening::{Happening as H, Step};
    let H::Expansion { task, step } = happening;
    match step {
        Step::Requested { paths } => format!(
            "{task} asks for {}",
            paths
                .iter()
                .map(|glob| glob.as_str().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Step::Granted { by } => format!("{task} scope granted by {}", decider(by)),
        Step::Denied { by, reason } => detailed(
            format!("{task} scope denied by {}", decider(by)),
            &one_line(reason.as_deref().unwrap_or_default()),
        ),
    }
}

/// Who settled a scope request.
fn decider(by: &yunta_core::events::Decider) -> String {
    match by {
        yunta_core::events::Decider::Rule => "the rule".to_string(),
        yunta_core::events::Decider::Person { id } => id.to_string(),
    }
}

fn finding_words(happening: &findings::happening::Happening) -> String {
    use findings::happening::{Change, Happening as H};
    let H::Finding {
        id,
        severity,
        title,
        change,
    } = happening;
    let named = match id {
        Some(id) => format!("finding {id}"),
        None => "a finding it did not name".to_string(),
    };
    let severity = severity
        .map(|severity| format!(" {}", severity.as_str()))
        .unwrap_or_default();
    match change {
        Change::Posted => detailed(format!("{named}{severity}"), &one_line(title)),
        Change::Updated => detailed(format!("{named} updated{severity}"), &one_line(title)),
        Change::Withdrawn { reason } => detailed(format!("{named} withdrawn"), &one_line(reason)),
        Change::Refused {
            operation,
            problems,
        } => format!(
            "{named} {} refused: {}",
            operation.as_str(),
            yunta_core::text::counted(*problems, "problem")
        ),
    }
}

fn artifact_words(happening: &artifacts::happening::Happening) -> String {
    use artifacts::happening::Happening as H;
    match happening {
        H::Submitted { kind, name, taken } => format!(
            "{} {name} submitted: {}",
            kind.as_str(),
            match taken {
                true => "accepted",
                false => "refused",
            }
        ),
        H::Accepted(id) => format!("accepted {id}"),
        H::Written { path } => format!("wrote {}", path.display()),
    }
}

fn gate_words(happening: &gates::happening::Happening) -> (Option<StateWord>, String) {
    use gates::happening::Happening as H;
    match happening {
        H::Escalated(payload) => (Some(StateWord::Wait), payload.summary().to_string()),
        H::Resolved(payload) => (Some(StateWord::Done), resolution(payload)),
        // The node's own label, so the chronicle and `status` say a
        // node that asked with the same bytes by construction.
        H::Asked { questions } => (
            Some(StateWord::Wait),
            match NonEmpty::new(questions.clone()) {
                Some(asked) => NodeDisplay::of(Some(&NodeState::Waiting {
                    on: NodeWait::Questions { asked },
                }))
                .label(),
                // A `questions_asked` naming nothing is a log this
                // binary never wrote — its constructor refuses one — so
                // the chronicle says what it has rather than a list it
                // would be inventing.
                None => StateWord::Wait.word().to_string(),
            },
        ),
        H::Answered { channel, responder } => (
            Some(StateWord::Done),
            match responder {
                Some(by) => format!("answered by {by} via {}", channel.as_str()),
                None => format!("answered via {}", channel.as_str()),
            },
        ),
    }
}

fn child_words(happening: &children::happening::Happening) -> (Option<StateWord>, String) {
    use children::happening::Happening as H;
    match happening {
        H::Born(run_id) => (Some(StateWord::Run), format!("child run {run_id} opened")),
        H::Closed { run_id, terminal } => (
            Some(StateWord::Done),
            format!("child run {run_id} {}", view::closed_as(*terminal)),
        ),
        H::Iteration { iteration } => (None, format!("iteration {iteration}")),
    }
}

/// What a session's fence covered, in one phrase — what says whether a
/// write that reaches the diff should have been possible at all.
fn fence_covered(coverage: &Coverage) -> String {
    match coverage {
        Coverage::Exact => "fence exact".to_string(),
        Coverage::WidenedToRoots { roots } => format!(
            "fence widened to {}",
            yunta_core::text::counted(roots.len(), "root")
        ),
        Coverage::ToolsOnly => "fence on tool calls".to_string(),
    }
}

/// How a gate was settled: who settled it and what they said, in each
/// of the shapes the persisted object spells.
fn resolution(payload: &GateResolvedPayload) -> String {
    match payload {
        GateResolvedPayload::Chosen(choice) => {
            format!("`{}` chosen by {}", choice.option, choice.by)
        }
        GateResolvedPayload::Approved { by, sha } => format!("approved by {by} over {sha}"),
        GateResolvedPayload::ChangesRequested { by } => format!("changes requested by {by}"),
        GateResolvedPayload::Closed => "closed without merging".to_string(),
        GateResolvedPayload::Unrecognized(_) => {
            "settled in a shape this binary does not name".to_string()
        }
    }
}
