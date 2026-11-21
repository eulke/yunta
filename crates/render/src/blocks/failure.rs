//! What a failure says beyond its node's row: the end of what a command
//! printed and where the rest is, the problems of each document it
//! refused, the paths it should not have touched. And the words of the
//! menu a failure offers: what each option does, what it waits for, and
//! why no option is offered when none is.

use std::path::{Path, PathBuf};

use surco::components::{Evidence, PathNote, Paths, Said, Whole};
use surco::doc::Block;
use yunta_core::events::{ExhaustedReroute, Failure, FailureMenu, Precondition, Stuck, TaskCause};
use yunta_core::protected::Protection;
use yunta_core::reserved::ReservedOption;
use yunta_core::{listed_globs, ModeName};

use crate::failure as words;

/// One failure's detail, as every surface that quotes it quotes it.
pub struct FailureDetail<'a> {
    pub failure: &'a Failure,
    /// Where the whole of what a command printed is: the file it is
    /// kept in, or the command that shows it.
    pub whole: Option<Whole>,
}

/// What a failure's detail says, whatever medium draws it.
enum FailureSays {
    /// The end of what a command printed, and where the rest is.
    Evidence(Evidence),
    /// Lines that say it whole.
    Text(Vec<String>),
    /// A heading, and each path it names.
    Listed {
        heading: String,
        paths: Vec<PathNote>,
    },
    /// Nothing beyond the node's own row.
    Nothing,
}

impl FailureDetail<'_> {
    /// The blocks this failure says itself in beyond its node's row, on
    /// every surface: the end of what a command printed, lines said whole,
    /// or a heading and the paths it names.
    pub fn blocks(&self) -> Vec<Block> {
        match self.says() {
            FailureSays::Evidence(evidence) => vec![Block::Evidence(evidence)],
            FailureSays::Text(lines) => vec![Block::Said(Said(lines))],
            FailureSays::Listed { heading, paths } => vec![Block::Paths(Paths { heading, paths })],
            FailureSays::Nothing => Vec::new(),
        }
    }

    /// What this failure says beyond its node's row.
    fn says(&self) -> FailureSays {
        let failure = self.failure;
        match failure {
            // A command that printed nothing has nothing to quote.
            Failure::Exited { exited } if exited.tail.is_empty() && self.whole.is_none() => {
                FailureSays::Nothing
            }
            Failure::Exited { exited } => FailureSays::Evidence(Evidence {
                tail: exited.tail.clone(),
                whole: self.whole.clone(),
                kept: None,
            }),
            Failure::SessionDied { died } => session_says(died),
            Failure::Artifacts { artifacts } => artifacts_says(artifacts),
            Failure::ScopeViolated { outside_scope } if outside_scope.len() > 1 => listed(
                "outside the declared globs:",
                outside_scope.iter().map(|path| plain(path)).collect(),
            ),
            Failure::PathsDenied {
                denied_paths,
                reasons,
            } if denied_paths.len() > 1 => denied_says(denied_paths, reasons),
            Failure::Message { outcome } if outcome.as_str().contains('\n') => {
                FailureSays::Text(lines_of(outcome.as_str()))
            }
            Failure::Unchanged { unchanged } => unchanged_says(unchanged.since, &unchanged.failure),
            Failure::ScopeOwed { .. }
            | Failure::Red { .. }
            | Failure::GateChose { .. }
            | Failure::ChildFailed {
                child_failure: Some(_),
                ..
            } => FailureSays::Text(words::detail(failure)),
            Failure::ScopeViolated { .. }
            | Failure::PathsDenied { .. }
            | Failure::Message { .. }
            | Failure::ScopeRequested { .. }
            | Failure::Unset { .. }
            | Failure::PlanUnpassable { .. }
            | Failure::SessionRefused { .. }
            | Failure::Limit { .. }
            | Failure::FrozenInput { .. }
            | Failure::SessionNotStopped { .. }
            | Failure::ChildFailed {
                child_failure: None,
                ..
            } => FailureSays::Nothing,
            // A failure told in one sentence: a loop's stops and the engine's own
            // words, whose lines show only when the sentence runs over more than one.
            _ => sentence_says(failure),
        }
    }
}

/// Each document that did not close, in the lines its own block takes.
fn artifacts_says(artifacts: &[yunta_core::ArtifactFailure]) -> FailureSays {
    FailureSays::Text(
        artifacts
            .iter()
            .flat_map(|artifact| lines_of(&artifact.to_string()))
            .collect(),
    )
}

/// The paths a denial names, each with the reason it is protected for when
/// the failure gives one: the reasons line up with the paths by index.
fn denied_says(paths: &[PathBuf], reasons: &[Protection]) -> FailureSays {
    listed(
        "what no session of the run may write:",
        paths
            .iter()
            .enumerate()
            .map(|(index, path)| PathNote {
                path: path.clone(),
                reason: reasons.get(index).map(|reason| reason.words().to_string()),
            })
            .collect(),
    )
}

/// The tail of a session that died with stderr, or nothing when it said none.
fn session_says(died: &yunta_core::events::SessionDeath) -> FailureSays {
    let tail: Vec<String> = died
        .exit
        .iter()
        .flat_map(|exit| exit.stderr_tail.clone())
        .collect();
    match tail.is_empty() {
        true => FailureSays::Nothing,
        false => FailureSays::Evidence(Evidence {
            tail,
            whole: None,
            kept: None,
        }),
    }
}

/// What an unchanged attempt says: why it did not run, and the lines of
/// the failure it repeats.
fn unchanged_says(since: u32, failure: &Failure) -> FailureSays {
    let mut lines = lines_of(&words::unchanged_sentence(since, &words::headline(failure)));
    lines.extend(words::detail(failure));
    FailureSays::Text(lines)
}

/// A failure told in one sentence: the lines of the sentence when it runs
/// over more than one line, and nothing beyond the row when it does not.
fn sentence_says(failure: &Failure) -> FailureSays {
    let said = words::sentence(failure);
    match said.contains('\n') {
        true => FailureSays::Text(lines_of(&said)),
        false => FailureSays::Nothing,
    }
}

/// A path with no reason beside it.
fn plain(path: &Path) -> PathNote {
    PathNote {
        path: path.to_path_buf(),
        reason: None,
    }
}

/// A heading and its paths.
fn listed(heading: &str, paths: Vec<PathNote>) -> FailureSays {
    FailureSays::Listed {
        heading: heading.to_string(),
        paths,
    }
}

/// The lines of `said`, without the line breaks that would split a row.
fn lines_of(said: &str) -> Vec<String> {
    said.lines().map(str::to_string).collect()
}

/// How a retry starts, which is the same whatever input it waits for.
const RETRY_STARTS: &str = "Starts a fresh attempt from the run's tree as it stands";

/// What a retry asks again and what it keeps, whatever input it waits for.
const RETRY_KEEPS: &str = "what failed is asked again and what passed stays passed";

/// What a failure's menu offers beside its abort, written from the menu's own facts: what
/// each option does and what it costs, either of which its name may say alone. `None` for
/// an option the menu does not write.
pub(crate) fn remedy_words(
    menu: &FailureMenu,
    option: ReservedOption,
) -> Option<(Option<String>, Option<String>)> {
    let tasks = stopped_tasks(&menu.failure);
    let departing = departing_tasks(&menu.failure);
    match option {
        ReservedOption::Retry if matches!(menu.failure, Failure::LoopCapped { .. }) => {
            Some(loop_retry_words())
        }
        ReservedOption::Retry => {
            let waits = retry_waits(menu)?;
            Some(retry_words(&waits))
        }
        ReservedOption::Grant => Some(grant_words(menu)),
        ReservedOption::Deny => Some((None, Some(crate::offers::goes_on_within(tasks)))),
        ReservedOption::ContinueWork => Some((None, Some(continue_words(tasks)))),
        ReservedOption::Continue => Some(past_the_cap_words(menu.attempt)),
        ReservedOption::Accept => Some((None, Some(super::decision::accepted(departing)))),
        ReservedOption::SendBack => Some((None, Some(super::decision::sent_back(departing)))),
        _ => None,
    }
}

/// How many tasks the failure's answers are about: those that owe a scope request, or that
/// a loop stopped on; one for any other failure.
fn stopped_tasks(failure: &Failure) -> usize {
    match failure {
        Failure::ScopeOwed { owed } => owed.len().max(1),
        Failure::TasksStopped { stopped } => stopped.len().max(1),
        _ => 1,
    }
}

/// How many of a loop's tasks the failure stopped on a departure from the plan; one for a
/// failure that stopped none, whose answers speak of the one task they are about.
fn departing_tasks(failure: &Failure) -> usize {
    match failure {
        Failure::TasksStopped { stopped } => stopped
            .iter()
            .filter(|stop| matches!(stop.cause, TaskCause::DeviationOwed { .. }))
            .count()
            .max(1),
        _ => 1,
    }
}

/// The words of running a capped loop again, chosen by its failure so that a menu an older
/// log holds, which waits for a person to lift the cap, says what a retry does.
fn loop_retry_words() -> (Option<String>, Option<String>) {
    (
        None,
        Some("its iteration count starts over; it stops at the cap again if tasks remain".into()),
    )
}

/// The words of running a node again past the token cap that cut its session.
fn past_the_cap_words(attempt: u32) -> (Option<String>, Option<String>) {
    (
        Some(format!("Run it again past the cap (attempt {attempt})")),
        Some(crate::offers::LIFTS_THE_CAP.to_string()),
    )
}

/// What the menu's retries wait for, each input named once and in the order
/// the remedies name it. `None` when the menu offers no retry, or a retry
/// that names no input to wait for.
fn retry_waits(menu: &FailureMenu) -> Option<Vec<Precondition>> {
    let mut waits: Vec<Precondition> = Vec::new();
    for remedy in menu
        .remedies
        .iter()
        .filter(|remedy| remedy.option == ReservedOption::Retry)
    {
        let precondition = remedy.precondition.as_ref()?;
        if !waits.contains(precondition) {
            waits.push(precondition.clone());
        }
    }
    (!waits.is_empty()).then_some(waits)
}

/// The words of a retry that waits for each of `waits`: what it starts from, what it asks
/// again, and what must change for it to end differently.
fn retry_words(waits: &[Precondition]) -> (Option<String>, Option<String>) {
    // A tree that already moved needs no waiting, so only the other inputs make
    // the clause, and with none left the retry waits for nothing.
    let waiting: Vec<String> = waits
        .iter()
        .filter(|precondition| **precondition != Precondition::TreeChanged)
        .map(waits_for)
        .collect();
    let costs = match waiting.is_empty() {
        true => RETRY_KEEPS.to_string(),
        false => format!(
            "{RETRY_KEEPS}; until {}, it fails the same way and asks again",
            and_list(&waiting)
        ),
    };
    (Some(RETRY_STARTS.to_string()), Some(costs))
}

/// What a retry waits for, as the clause what it costs says after "until".
fn waits_for(precondition: &Precondition) -> String {
    match precondition {
        Precondition::TreeChanged => "the tree changes".to_string(),
        Precondition::OutsideTreeChanged => "something outside the tree changes".to_string(),
        Precondition::CredentialFixed => "the credential is fixed".to_string(),
        Precondition::ModelAvailable => "the model it asked for is available".to_string(),
        Precondition::LimitLifted => "the service accepts requests again".to_string(),
        Precondition::ServiceReachable => "the service is reachable".to_string(),
        Precondition::CommandRunnable { cmd } => format!("`{cmd}` can run"),
        Precondition::CapLifted => "a person lifts the cap on resume".to_string(),
    }
}

/// Clauses read as one phrase: "a", "a and b", or "a, b, and c".
fn and_list(clauses: &[String]) -> String {
    match clauses {
        [] => String::new(),
        [only] => only.clone(),
        [first, last] => format!("{first} and {last}"),
        [init @ .., last] => format!("{}, and {last}", init.join(", ")),
    }
}

/// The words of a grant: the paths it adds to the node's scope, and the
/// attempt it starts. A loop that ended owing its tasks' requests widens
/// each task by its own request instead, which the card's question names.
fn grant_words(menu: &FailureMenu) -> (Option<String>, Option<String>) {
    if let Failure::ScopeOwed { owed } = &menu.failure {
        let paths = owed.iter().map(|asked| asked.paths.len()).sum();
        return (None, Some(crate::offers::may_also_touch(owed.len(), paths)));
    }
    (
        Some(format!(
            "Allow the node to also write {} and run it again (attempt {})",
            wanted(&menu.failure),
            menu.attempt
        )),
        Some(
            "Adds these paths to the node's scope for the rest of this run; the new attempt \
             starts fresh, fenced and judged against the widened scope"
                .to_string(),
        ),
    )
}

/// The paths a grant would add: what the failure says its work wrote
/// outside the scope, or what its session asked for.
fn wanted(failure: &Failure) -> String {
    match failure {
        Failure::ScopeRequested { requested_scope } => listed_globs(&requested_scope.paths),
        Failure::ScopeViolated { outside_scope } => words::paths_listed(outside_scope),
        Failure::Unchanged { unchanged } => wanted(&unchanged.failure),
        _ => String::new(),
    }
}

/// What a continuation from the work a loop's blocked task, or each of `tasks`, left does.
fn continue_words(tasks: usize) -> String {
    let whose = match tasks {
        1 => "the blocked task's",
        _ => "each blocked task's",
    };
    format!("keeps {whose} last changes and judges them before any session opens")
}

/// The words of the options a reroute the failure exhausted offers: the
/// retry through the corrective node once more, and the promotion. `None`
/// for an option the reroute does not write.
pub(crate) fn reroute_words(
    reroute: &ExhaustedReroute,
    option: ReservedOption,
) -> Option<(Option<String>, Option<String>)> {
    match option {
        ReservedOption::Retry => Some((
            Some(format!("Reroute to `{}` once more", reroute.goto)),
            Some(format!(
                "Uses one extra correction attempt beyond the declared max_reroutes ({}); \
                 escalates again if `{}` doesn't fix it",
                reroute.max_reroutes, reroute.goto
            )),
        )),
        ReservedOption::Promote => Some(promotion_words(reroute.next_mode.as_ref())),
        _ => None,
    }
}

/// The words of a promotion to `next_mode`, which a reroute names when the
/// workflow declares a mode after this one.
fn promotion_words(next_mode: Option<&ModeName>) -> (Option<String>, Option<String>) {
    let (label, target) = match next_mode {
        Some(mode) => (format!("Promote to mode `{mode}`"), format!("`{mode}`")),
        None => (
            "Promote to the next mode".to_string(),
            "the next mode".to_string(),
        ),
    };
    (
        Some(label),
        Some(format!(
            "Closes this run (`run_finished: promoted`) and starts a successor in {target}, \
             inheriting this run's artifacts; there's no mechanism to demote back to the mode \
             this run is in"
        )),
    )
}

/// Why no option is offered, as the way out a person reads.
pub(crate) fn stuck_words(stuck: &Stuck) -> String {
    match stuck {
        Stuck::FrozenConfig => "this run's config was frozen when it was created, so no attempt \
            of it can go differently: declare what is missing in the config and start a new run"
            .to_string(),
        Stuck::FrozenInput => "a file this run froze when it was created has changed since, and \
            no attempt of it can read the file as it was: start a new run to read it as it \
            now stands"
            .to_string(),
        Stuck::ProjectDenies => "the project's permissions refuse it, so no attempt goes \
            differently: change what they refuse, or the permissions themselves"
            .to_string(),
        Stuck::NoGrant => "this run does not grant the scope the work needs, so no attempt goes \
            differently: change the work to stay inside the node's scope"
            .to_string(),
        Stuck::TreeUnchanged => "the same check on the same tree answers the same: change what it \
            failed on in the run's tree, then resume the run, and the wake that finds the change \
            offers to run it again"
            .to_string(),
        Stuck::Owed => "a person's decision is owed before the node can go on: answer it, and the \
            run goes on from there"
            .to_string(),
        Stuck::Final => "the session refused the request for a reason no new attempt changes, so \
            every attempt is refused the same way"
            .to_string(),
        Stuck::CriteriaWrong => "a declared criterion no work of the plan can pass: the plan has \
            to change first"
            .to_string(),
    }
}
