//! The facts a red output locates: a path the output names as `path:line`,
//! and the criterion that printed it. A refused write, or a request citing one
//! of the task's own criteria as its evidence, is located this way, and the
//! judgement decides on those facts through `scope_expansion::expand`. The
//! facts are what the engine verified, so nobody is asked to confirm them.

use std::path::Path;

use yunta_core::events::ToolTarget;
use yunta_core::expansion::Precheck;
use yunta_core::ScopeGlob;

use super::criteria::could_not_run;
use super::judge::{Deciding, Refusals, Sitting};
use super::{CriterionRun, SessionObserver, TaskCycleError};
use crate::artifacts::store::ObjectStore;
use crate::process::Printed;
use crate::scope_expansion::{locates, relative_text, ScopeExpansionRequest};

/// Where the output a criterion kept is read from, when its answer was reused
/// rather than run: the observer that holds it, or the run's directory.
pub(super) enum Kept<'a> {
    Observer(&'a dyn SessionObserver),
    Run(&'a Path),
    Nowhere,
}

/// What a red output locates, owned so that a decision can borrow it: the
/// paths, the criterion that located them, the request made (none for a
/// refused write) and the pre-check of that request's criterion.
pub(super) struct Located {
    pub(super) paths: Vec<ScopeGlob>,
    pub(super) criterion: String,
    pub(super) asked: Option<ScopeExpansionRequest>,
    pub(super) precheck: Option<Precheck>,
}

/// Located facts, when there is at least one path. Nothing located decides nothing.
pub(super) fn located(
    paths: Vec<ScopeGlob>,
    criterion: String,
    asked: Option<ScopeExpansionRequest>,
    precheck: Option<Precheck>,
) -> Option<Located> {
    (!paths.is_empty()).then_some(Located {
        paths,
        criterion,
        asked,
        precheck,
    })
}

/// A proposed criterion's exit on the checkout, read through the memo as the
/// request's own pre-check.
pub(super) async fn precheck_of(
    deciding: &Deciding<'_>,
    cmd: &str,
) -> Result<Precheck, TaskCycleError> {
    let exit = deciding
        .env
        .memo
        .exit_code(cmd, deciding.checkout, deciding.env.supervision)
        .await?
        .exit_code;
    Ok(match exit {
        0 => Precheck::Passes,
        exit if could_not_run(exit).is_some() => Precheck::CouldNotRun,
        exit => Precheck::Red { exit },
    })
}

/// The refused writes a red criterion may locate. Only a task reads them, and
/// only while a criterion is red: a refused write explains only a failure.
pub(super) async fn refused_writes(
    sitting: &Sitting<'_>,
    runs: &[CriterionRun],
) -> Result<Vec<ToolTarget>, TaskCycleError> {
    let Some(task) = sitting.task.filter(|_| runs.iter().any(is_red)) else {
        return Ok(Vec::new());
    };
    match &sitting.refused {
        Refusals::None => Ok(Vec::new()),
        Refusals::Observer {
            observer,
            node,
            sessions,
        } => super::stream::read_refusals(Some((*observer, *node)), sessions, runs)
            .await
            .map_err(|source| TaskCycleError::Audit {
                task: task.id.clone(),
                source,
            }),
        Refusals::Log {
            events, sessions, ..
        } => Ok(crate::run_tools::refused_writes(events, sessions)),
    }
}

/// The first red criterion of `runs` whose output `locate` finds facts in. The
/// output is read as it stands relative to the checkout, and is only read until
/// a criterion locates something.
pub(super) async fn first_located(
    deciding: &Deciding<'_>,
    runs: &[CriterionRun],
    locate: impl Fn(&str, &str) -> Option<Located>,
) -> Option<Located> {
    for run in runs {
        let Some(printed) = red_output(run) else {
            continue;
        };
        let text = text_of(&deciding.kept, deciding.checkout, printed).await;
        if let Some(found) = locate(text.as_str(), run.cmd.as_str()) {
            return Some(found);
        }
    }
    None
}

/// Everything `printed` says, with the checkout's own path taken out, so a
/// location reads relative to it whichever way the command printed it. An answer
/// whose kept object cannot be read is read from the last lines the log quotes.
pub(super) async fn text_of(kept: &Kept<'_>, checkout: &Path, printed: &Printed) -> String {
    let text = match printed {
        Printed::Ran(output) => String::from_utf8_lossy(output.bytes()).into_owned(),
        Printed::Recorded { object, tail, .. } => {
            let kept = match (object, kept) {
                (Some(hash), Kept::Observer(observer)) => observer.kept_output(hash).await,
                (Some(hash), Kept::Run(run_dir)) => ObjectStore::at(run_dir).get(hash).await.ok(),
                _ => None,
            };
            match kept {
                Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                None => tail.join("\n"),
            }
        }
    };
    relative_text(text, checkout)
}

/// Whether `text` locates every path of `paths`.
pub(super) fn locates_every(text: &str, paths: &[ScopeGlob]) -> bool {
    paths.iter().all(|path| locates(text, path.as_str()))
}

/// Whether a criterion ran red: it exited non-zero, and it did answer.
pub(super) fn is_red(run: &CriterionRun) -> bool {
    run.exit_code != 0 && run.could_not_run().is_none()
}

/// What a red criterion printed, when it kept what it printed.
fn red_output(run: &CriterionRun) -> Option<&Printed> {
    run.output.as_ref().filter(|_| is_red(run))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kept answer whose location sits above its tail is located from the
    /// output the object holds, and from the tail alone it is not.
    #[tokio::test]
    async fn a_kept_answer_locates_from_the_output_it_kept() {
        let dir = tempfile::tempdir().unwrap();
        let output =
            b"error[E0061]: this takes one argument\n  --> b.txt:3:9\nmany lines\nlast line\n";
        let object = ObjectStore::at(dir.path()).put(output).await.unwrap();
        let printed = Printed::Recorded {
            object: Some(object),
            tail: vec!["last line".to_string()],
            conclusion: None,
        };
        let kept = Kept::Run(dir.path());
        assert!(locates(
            &text_of(&kept, dir.path(), &printed).await,
            "b.txt"
        ));
        assert!(!locates(
            &text_of(&Kept::Nowhere, dir.path(), &printed).await,
            "b.txt"
        ));
    }
}
