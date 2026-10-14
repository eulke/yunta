//! The sentences a session reads back from the run's tools: what a refused
//! call says, what a task's session is told first, what the protocol answers
//! to a tool it was never offered, and how a plan's known items read.

use yunta_core::{FindingId, NodeId, Task};

use crate::run_tools::RunToolError;

/// The text a refused call reads back: the sentence the variant says,
/// then each cause behind it that the sentence does not already state.
pub(crate) fn tool_error(error: &RunToolError) -> String {
    yunta_core::with_causes(said(error), error)
}

/// The sentence a refused call reads, ahead of the causes behind it.
fn said(error: &RunToolError) -> String {
    match error {
        RunToolError::Refused { text } => text.clone(),
        RunToolError::Unredactable { kind } => unredactable(kind),
        RunToolError::InvalidSubmission { names, detail } => invalid_submission(names, detail),
        RunToolError::InvalidRequest { .. } => invalid_request(),
        RunToolError::InvalidDeparture { .. } => invalid_departure(),
        RunToolError::InvalidAnswer { .. } => invalid_answer(),
        RunToolError::EmptyAnswer => empty_answer(),
        RunToolError::OwnFinding {
            id,
            update,
            withdraw,
        } => own_finding(id, update, withdraw),
        RunToolError::SettledFinding { node, id, settled } => settled_finding(node, id, settled),
        RunToolError::NoSuchFinding { node, id } => no_such_finding(node, id),
        RunToolError::WithdrawnFinding { node, id, reason } => withdrawn_finding(node, id, reason),
        RunToolError::EmptyDeparture { field } => empty_departure(field),
        RunToolError::NotInThePlan { from, known } => not_in_the_plan(from, known),
        RunToolError::SuiteDeparture { cmd } => suite_departure(cmd),
        RunToolError::NotInBlackboardGroup => not_in_blackboard_group(),
        RunToolError::NoScopeToWiden => no_scope_to_widen(),
        RunToolError::NoNodeScope => no_node_scope(),
        RunToolError::NoStartingTree => no_starting_tree(),
        RunToolError::CriterionConflict { pending } => criterion_conflict(pending),
        RunToolError::NotRedOnTheWork { criterion, exit } => not_red_on_the_work(criterion, *exit),
        RunToolError::Criterion { .. } => criterion_not_run(),
        RunToolError::Record { .. } => scope_not_recorded(),
        RunToolError::NotATaskSession { tool } => not_a_task_session(tool),
        RunToolError::Handover { detail } => handover(detail),
        RunToolError::Plan { .. } => plan_unreadable(),
        RunToolError::Check { .. } => work_unchecked(),
        RunToolError::Superseded => superseded(),
        RunToolError::Cancelled => "cancelled by user".to_string(),
        RunToolError::Storage { .. } => log_unreachable(),
        RunToolError::Render { .. } => not_rendered(),
        RunToolError::Request { source } => source.to_string(),
        RunToolError::NoArtifacts { node } => no_artifacts(node),
        RunToolError::UndeclaredArtifact { name, declared } => undeclared_artifact(name, declared),
        RunToolError::Accept { source } => source.to_string(),
    }
}

fn unredactable(kind: &str) -> String {
    format!(
        "nothing was recorded: a value this call carries cannot be kept out of the {kind} record"
    )
}

fn invalid_submission(names: &str, detail: &str) -> String {
    format!(
        "invalid submission — requires `document` (an object), the {names} document this node \
         declares: {detail}"
    )
}

fn invalid_request() -> String {
    "invalid request — requires paths (list) and reason, with an optional proposed_criterion \
     {cmd} or evidence (one of your task's criteria)"
        .to_string()
}

fn invalid_departure() -> String {
    "invalid declaration — requires `kind`: `departure`, with `from` (one of \
     `{\"shape\": name}`, `{\"decision\": id}`, `{\"change\": at}`, `{\"criterion\": cmd}` or \
     `\"outcome\"`), `planned`, `instead` and `why`; or `unpassable`, with `criterion` and `why`"
        .to_string()
}

fn invalid_answer() -> String {
    "invalid answer — requires `node` and `id`, the finding answered as its node reported it, \
     `answer` (`fixed` or `declined`) and `why`"
        .to_string()
}

fn empty_answer() -> String {
    "an answer says why — `why` is empty".to_string()
}

fn own_finding(id: &FindingId, update: &str, withdraw: &str) -> String {
    format!(
        "`{id}` is a finding this node reported: change it with `{update}`, or take it back \
         with `{withdraw}`, saying why"
    )
}

fn settled_finding(node: &NodeId, id: &FindingId, settled: &str) -> String {
    format!("node `{node}`'s finding `{id}` is settled — {settled}; there is nothing to answer")
}

fn no_such_finding(node: &NodeId, id: &FindingId) -> String {
    format!("node `{node}` reported no finding `{id}` on this run")
}

fn withdrawn_finding(node: &NodeId, id: &FindingId, reason: &str) -> String {
    format!("node `{node}` took its finding `{id}` back — {reason}; there is nothing to answer")
}

fn empty_departure(field: &str) -> String {
    format!("a declaration says what it is — `{field}` is empty")
}

fn not_in_the_plan(from: &str, known: &str) -> String {
    format!("the plan holds no {from} — it holds {known}")
}

fn suite_departure(cmd: &str) -> String {
    format!(
        "`{cmd}` is a command the run measured green before any work, which holds every task — \
         it is not the plan's to depart from: keep what passed passing"
    )
}

fn not_in_blackboard_group() -> String {
    "this session's node is not in a `coordination: blackboard` group — the blackboard is never \
     mounted outside one"
        .to_string()
}

fn no_scope_to_widen() -> String {
    "this session has no scope an answer could widen — it works no task, and its node either \
     declares no scope or may not be granted more on this run"
        .to_string()
}

fn no_node_scope() -> String {
    "this tool judges a node's own scope, and this session's node declares none".to_string()
}

fn no_starting_tree() -> String {
    "this attempt's start is not on the log, so there is no tree to judge its work from".to_string()
}

fn criterion_conflict(pending: &str) -> String {
    format!(
        "a scope expansion request under `{pending}` is already waiting for this attempt's end, \
         and only requests under the same criterion join it"
    )
}

fn not_red_on_the_work(criterion: &str, exit: i32) -> String {
    format!(
        "`{criterion}` does not fail on the work (exit {exit}), so it cannot be declared \
         unpassable — only a criterion that is red on the work can be"
    )
}

fn criterion_not_run() -> String {
    "the criterion could not be run to see whether it is red on the work".to_string()
}

fn scope_not_recorded() -> String {
    "the scope decision could not be recorded on the run's log".to_string()
}

fn not_a_task_session(tool: &str) -> String {
    format!(
        "`{tool}` answers about a task, and this session works none — only a loop's task \
         sessions are served it"
    )
}

fn handover(detail: &str) -> String {
    format!("the criteria could not be run where the engine runs them: {detail}")
}

fn plan_unreadable() -> String {
    "the run's plan could not be read".to_string()
}

fn work_unchecked() -> String {
    "the task's work could not be checked".to_string()
}

fn superseded() -> String {
    "this check stopped: the checkout changed, and the check asked after it judges what the \
     checkout holds now — its answer is the one that counts"
        .to_string()
}

fn log_unreachable() -> String {
    "the run's log cannot be reached".to_string()
}

fn not_rendered() -> String {
    "the answer cannot be rendered as JSON".to_string()
}

fn no_artifacts(node: &NodeId) -> String {
    format!("node `{node}` declares no artifacts, so there is nothing to check")
}

fn undeclared_artifact(name: &str, declared: &str) -> String {
    format!("`{name}` is not an artifact this node declares; it declares {declared}")
}

/// What the protocol answers to a call for a tool this session was never offered.
pub(crate) fn unknown_tool(name: &str) -> String {
    format!("unknown tool `{name}`")
}

/// The items a plan holds of one kind, as a refusal names them: the items
/// listed, or that the plan holds none of that kind.
pub(crate) fn known_list(known: &[&str]) -> String {
    match known {
        [] => "nothing of that kind".to_string(),
        _ => yunta_core::text::listed(known.iter().copied()),
    }
}

/// What a task's session is told first: the instruction it was given, and
/// which task it works. The task's own contract is read through its tools.
pub(crate) fn task_brief(instruction: &str, task: &Task) -> String {
    format!(
        "{instruction}\n\nYour task: `{}` — {}.",
        task.id, task.title
    )
}
