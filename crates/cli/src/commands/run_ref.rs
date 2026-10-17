//! The run a person means by what they typed where a command takes one.
//!
//! A run is called by its handle — the end of its id — on every human
//! line, so a command takes the handle back, and with it any part of the
//! id that starts or ends it, the whole id, and two words: `last`, this
//! repository's newest run, and `needs`, the one of its runs waiting on
//! a person. A part two runs share is refused naming both, never guessed.

use std::str::FromStr;

use yunta_core::RunId;

use super::list::inbox::Inbox;
use crate::context::Context;
use crate::error::CliError;

/// What a person typed where a command takes a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunRef {
    /// `last`: the newest run of the repository this is run in.
    Last,
    /// `needs`: the run of the repository this is run in that waits on
    /// a person.
    Needs,
    /// An id, or a part of one that starts or ends it.
    Typed(String),
}

impl FromStr for RunRef {
    type Err = String;

    fn from_str(typed: &str) -> Result<Self, Self::Err> {
        match typed.trim() {
            "" => Err("a run is named by its id, a part of it, `last` or `needs`".to_string()),
            "last" => Ok(RunRef::Last),
            "needs" => Ok(RunRef::Needs),
            typed => Ok(RunRef::Typed(typed.to_string())),
        }
    }
}

/// A run a reference could mean, and what tells it apart from the
/// others it could mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) run_id: RunId,
    pub(crate) about: Option<String>,
}

/// The run a command takes, as every command that takes one documents
/// it.
#[derive(Debug, Clone, clap::Args)]
pub(crate) struct RunArg {
    /// The run: its handle, its id or any part that starts or ends it,
    /// `last` (this repository's newest) or `needs` (the one waiting on
    /// you).
    run: RunRef,
}

impl RunArg {
    /// The run this names, among the runs this machine holds.
    pub(crate) async fn named(&self) -> Result<RunId, CliError> {
        self.run.resolve(&Context::load()?).await
    }
}

/// The run `reference` names, when a command was given one.
pub(crate) async fn named(reference: Option<&RunRef>) -> Result<Option<RunId>, CliError> {
    match reference {
        Some(reference) => Ok(Some(reference.resolve(&Context::load()?).await?)),
        None => Ok(None),
    }
}

impl RunRef {
    /// The run this reference means, among the runs under `ctx`'s state
    /// root.
    pub(crate) async fn resolve(&self, ctx: &Context) -> Result<RunId, CliError> {
        match self {
            RunRef::Typed(typed) => typed_run(ctx, typed),
            RunRef::Last => Inbox::gather(ctx, false)
                .await?
                .newest()
                .cloned()
                .ok_or_else(|| {
                    CliError::msg(
                        "no run in this repository — `yunta list --runs --all` lists every run",
                    )
                }),
            RunRef::Needs => waiting_run(ctx).await,
        }
    }
}

/// The run `typed` names among every run under `ctx`'s state root.
///
/// Text that matches no run is handed on as the id it would be, so the
/// command that takes it says what it says of a run that does not exist.
fn typed_run(ctx: &Context, typed: &str) -> Result<RunId, CliError> {
    let runs = ctx.storage()?.list_runs()?;
    let ids: Vec<RunId> = runs.into_iter().map(|run| run.run_id).collect();
    match matching(typed, &ids) {
        Matched::One(run_id) => Ok(run_id),
        Matched::None => {
            RunId::try_from(typed.to_string()).map_err(|error| CliError::msg(error.to_string()))
        }
        Matched::Several(candidates) => Err(CliError::AmbiguousRun {
            said: format!("`{typed}` is part of {} ids", candidates.len()),
            candidates: candidates
                .into_iter()
                .map(|run_id| Candidate {
                    run_id,
                    about: None,
                })
                .collect(),
        }),
    }
}

/// The one run of the repository `ctx` is in that waits on a person.
async fn waiting_run(ctx: &Context) -> Result<RunId, CliError> {
    let inbox = Inbox::gather(ctx, false).await?;
    match inbox.needing_a_person().as_slice() {
        [] => Err(CliError::msg(
            "no run of this repository needs you — `yunta list --runs` lists them",
        )),
        [one] => Ok(one.run_id.clone()),
        several => Err(CliError::AmbiguousRun {
            said: format!(
                "{} of this repository need you",
                yunta_core::text::counted(several.len(), "run")
            ),
            candidates: several
                .iter()
                .map(|row| Candidate {
                    run_id: row.run_id.clone(),
                    about: Some(format!("{} ({})", row.workflow, row.mode)),
                })
                .collect(),
        }),
    }
}

/// What a typed text matches among `ids`.
#[derive(Debug, PartialEq, Eq)]
enum Matched {
    None,
    One(RunId),
    Several(Vec<RunId>),
}

/// The runs among `ids` that `typed` names: the one it spells whole,
/// or every one it starts or ends, letters in either case.
fn matching(typed: &str, ids: &[RunId]) -> Matched {
    let typed = typed.to_ascii_uppercase();
    let upper = |id: &RunId| id.as_str().to_ascii_uppercase();
    if let Some(whole) = ids.iter().find(|id| upper(id) == typed) {
        return Matched::One(whole.clone());
    }
    let mut parts: Vec<RunId> = ids
        .iter()
        .filter(|id| {
            let id = upper(id);
            id.starts_with(&typed) || id.ends_with(&typed)
        })
        .cloned()
        .collect();
    parts.sort();
    match parts.len() {
        0 => Matched::None,
        1 => parts.pop().map_or(Matched::None, Matched::One),
        _ => Matched::Several(parts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "01K3W48MFW7H0ZZA5PZ07E5PH4";
    const SECOND: &str = "01K3W48MFW7H0ZZA5PZ0QQ12AB";
    const OTHER_DAY: &str = "01K2AAAAAAAAAAAAAAAAAA12AB";

    fn ids() -> Vec<RunId> {
        [FIRST, SECOND, OTHER_DAY]
            .into_iter()
            .map(RunId::from_static)
            .collect()
    }

    #[test]
    fn a_run_is_found_by_its_handle_a_prefix_and_its_full_id() {
        let first = Matched::One(RunId::from_static(FIRST));
        assert_eq!(matching("7E5PH4", &ids()), first);
        assert_eq!(matching("7e5ph4", &ids()), first, "either case");
        assert_eq!(matching(FIRST, &ids()), first);
        assert_eq!(
            matching("01K2", &ids()),
            Matched::One(RunId::from_static(OTHER_DAY))
        );
    }

    #[test]
    fn an_ambiguous_prefix_lists_every_run_it_matches() {
        assert_eq!(
            matching("01K3", &ids()),
            Matched::Several(vec![RunId::from_static(FIRST), RunId::from_static(SECOND)])
        );
        // A tail two runs share is as ambiguous as a head.
        assert!(matches!(matching("12AB", &ids()), Matched::Several(runs) if runs.len() == 2));
    }

    #[test]
    fn a_text_no_run_starts_or_ends_with_matches_none() {
        assert_eq!(matching("ZZZZZZ", &ids()), Matched::None);
        assert_eq!(matching("W48MFW", &ids()), Matched::None, "not the middle");
    }

    #[test]
    fn last_and_needs_are_words_and_anything_else_is_typed() {
        assert_eq!("last".parse(), Ok(RunRef::Last));
        assert_eq!("needs".parse(), Ok(RunRef::Needs));
        assert_eq!("7E5PH4".parse(), Ok(RunRef::Typed("7E5PH4".to_string())));
        assert!(" ".parse::<RunRef>().is_err());
    }
}
