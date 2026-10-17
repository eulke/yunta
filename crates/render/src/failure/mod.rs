//! The words a failure is told in: its headline, what it says beyond the
//! headline, and the sentence a log and `--json` read it as. Every surface
//! that says why a node stopped calls these, so none of them can say a
//! different thing. Each family of failures keeps its words in its own file.

mod artifact;
mod child;
mod command;
mod engine;
mod gate;
mod loops;
mod scope;
mod session;
mod setup;

use yunta_core::events::{ChildRefusal, Fact, Failure, RedCheck};

pub use artifact::{mount, name, object};
pub use command::{command_exit, command_headline, denied};
pub(crate) use scope::{paths_listed, protection_words};
pub(crate) use session::{limit_sentence, refusal_sentence, unchanged_sentence};
pub use session::{session_death, session_end};

/// The claim a failure is told by: what a surface states first, ahead of
/// what the failure says beyond it. One line, except for a loop, whose
/// claim is the list of tasks that stopped it.
pub fn headline(failure: &Failure) -> String {
    match failure {
        Failure::Exited { exited } => command_headline(exited),
        Failure::Unchanged { unchanged } => {
            unchanged_sentence(unchanged.since, &headline(&unchanged.failure))
        }
        Failure::Artifacts { artifacts } => artifact::headline(artifacts),
        // How many files, when there are several to list.
        Failure::ScopeViolated { outside_scope } if outside_scope.len() > 1 => {
            scope::violated_headline(outside_scope)
        }
        Failure::Red { red } => command::red_headline(red),
        Failure::ChildFailed {
            child_run,
            child_failure: Some(child),
        } => child::failed_headline(child_run, child),
        Failure::ChildRefused { workflow, refusal } => match refusal.as_ref() {
            ChildRefusal::FailsCheck { rules } => child::fails_check(workflow, rules),
            _ => sentence(failure),
        },
        Failure::GateChose { option, target, .. } => gate::gate_chose_headline(option, target),
        _ => sentence(failure),
    }
}

/// What the failure says beyond its [`headline`], a line each: the last
/// lines a command printed, the problems of a document that did not close
/// (each document's own block when several did), every path that fell
/// outside a scope when there are several, the lines of a failed child run,
/// and what a person said when they chose a gate's option. Nothing else: the
/// headline says the rest.
pub fn detail(failure: &Failure) -> Vec<String> {
    match failure {
        Failure::Exited { exited } => exited.tail.clone(),
        Failure::Unchanged { unchanged } => detail(&unchanged.failure),
        Failure::Artifacts { artifacts } => artifact::detail(artifacts),
        Failure::ScopeViolated { outside_scope } if outside_scope.len() > 1 => {
            scope::violated_detail(outside_scope)
        }
        Failure::ScopeOwed { owed } => owed.iter().map(scope::owed_line).collect(),
        Failure::Red {
            red:
                RedCheck::Command {
                    conclusion: Some(said),
                    ..
                },
        } => lines_of(said),
        Failure::Red {
            red: RedCheck::Regression { tail, .. },
        } => tail.clone(),
        Failure::ChildFailed {
            child_failure: Some(child),
            ..
        } => detail(child),
        Failure::GateChose {
            said: Some(said), ..
        } => lines_of(said),
        _ => Vec::new(),
    }
}

/// The lines of `said`, one entry each.
fn lines_of(said: &str) -> Vec<String> {
    said.lines().map(str::to_string).collect()
}

/// The failure as one sentence, the way a log and `--json` read it: every
/// line a command printed included, and none of it left out. One arm for
/// each variant, so a new variant must say its words here.
/// The cause a re-route carries, in the words a reader sees. A gate's choice
/// names the gate that made it; every other cause reads as its sentence.
pub fn cause(failure: &Failure) -> String {
    match failure {
        Failure::GateChose { .. } => gate::gate_choice_cause(failure),
        _ => sentence(failure),
    }
}

/// The failure in the words a reader sees.
pub fn sentence(failure: &Failure) -> String {
    match failure {
        Failure::Message { outcome } => outcome.as_str().to_string(),
        Failure::Artifacts { artifacts } => artifact::artifacts_said(artifacts),
        Failure::SessionDied { died } => session_death(died),
        Failure::ScopeViolated { outside_scope } => scope::violated(outside_scope),
        Failure::ScopeRequested { requested_scope } => scope::requested(requested_scope),
        Failure::ScopeOwed { owed } => scope::owed(owed),
        Failure::Unset { unset } => unset.to_string(),
        Failure::Unchanged { unchanged } => session::unchanged_said(unchanged),
        Failure::PathsDenied { .. } => scope::denied(failure),
        Failure::Exited { exited } => command_exit(exited),
        Failure::PlanUnpassable { unpassable } => loops::plan(unpassable),
        Failure::SessionRefused { session_refused } => refusal_sentence(session_refused),
        Failure::Limit { limit } => limit_sentence(limit),
        Failure::TasksStopped { stopped } => loops::tasks_stopped(stopped),
        Failure::Red { red } => command::red_sentence(red),
        Failure::ChildFailed { .. } => child::ended(failure),
        Failure::FrozenInput { frozen_input } => setup::frozen_input(frozen_input),
        Failure::SessionNotStopped { .. } => session::not_stopped(failure),
        Failure::TimedOut { after_ms, timed } => session::timed_out(*after_ms, timed),
        Failure::TokenBudget { used, max } => session::token_budget(*used, *max),
        Failure::Template { template, input } => artifact::template(template, input),
        Failure::ArtifactUnreadable { artifact, problem } => artifact::bytes(artifact, problem),
        Failure::ArtifactName { name, problem } => artifact::name(name, problem),
        Failure::MountUnreachable { error } => artifact::mount(error),
        Failure::GateChose { .. } => gate::gate_chose(failure),
        Failure::TestsAccepted { task, owed_by } => gate::tests_accepted(task, owed_by),
        Failure::ChildRefused { workflow, refusal } => child::refused(workflow, refusal),
        Failure::NestedTooDeep { .. } => child::nested_too_deep(failure),
        Failure::GateArtifactMissing {
            node,
            gate_artifact,
        } => artifact::gate_missing(node, gate_artifact),
        Failure::CapabilityMissing { .. } => setup::capability_missing(failure),
        Failure::EngineFailed { engine_error } => engine::failed(engine_error),
        Failure::CommandDenied { denied_rule } => command::denied(denied_rule),
        Failure::SkillMissing { .. } => setup::skill_missing(failure),
        Failure::LoopCapped { node, loop_cap } => loops::loop_capped(node, *loop_cap),
        Failure::Confinement { confinement } => confinement.to_string(),
        Failure::FrozenRead { frozen_read } => setup::frozen_read(frozen_read),
        Failure::ChildrenFailed { .. } => child::children_failed(failure),
        Failure::Uncertain { uncertain } => child::uncertain(uncertain),
        Failure::Rejected { on, .. } => gate::rejected(on),
        Failure::SentBack { sent_back } => gate::sent_back(sent_back),
        Failure::FindingsOpen { .. } => gate::findings_open(failure),
        Failure::Interrupted { .. } => child::interrupted(failure),
        Failure::ChangesRequested { .. } => gate::changes_requested(failure),
        Failure::PullRequestClosed { .. } => gate::pull_request_closed(failure),
        Failure::LoopStalled { .. } => loops::loop_stalled(),
        Failure::NoTasksDocument { .. } => loops::no_tasks_document(),
        Failure::RunnerUnresolved { .. } => setup::runner_unresolved(failure),
    }
}

/// Every fact a failure is recorded with beside its menu: its headline,
/// then each line of its detail, each a fact of its own.
pub fn facts(failure: &Failure) -> Vec<Fact> {
    std::iter::once(Fact::bare(headline(failure)))
        .chain(detail(failure).into_iter().map(Fact::bare))
        .collect()
}
