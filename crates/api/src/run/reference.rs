//! See [`super`]. The run a person means by what they typed where an
//! operation takes one.
//!
//! A run is called by its handle — the end of its id — on every human
//! line, so a reference takes the handle back, and with it any part of the
//! id that starts or ends it, the whole id, and two words: `last`, this
//! repository's newest run, and `needs`, the one of its runs waiting on
//! a person. A part two runs share is refused naming both, never guessed.

use std::str::FromStr;

use yunta_core::RunId;
use yunta_render::refusal::{Ambiguity, Candidate, Refused};
use yunta_storage::Storage;

use crate::{Error, Failure, FieldReach, Project, Stop};

/// What a person typed where an operation takes a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunRef {
    /// `last`: the newest run of the repository the project is rooted in.
    Last,
    /// `needs`: the run of the repository the project is rooted in that
    /// waits on a person.
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

impl Project {
    /// The id of the run `reference` means, among the runs this project's
    /// store holds.
    pub async fn find(&self, reference: &RunRef, stop: &Stop) -> Result<RunId, Error> {
        match reference {
            RunRef::Typed(typed) => self.typed_run(typed, stop).await,
            RunRef::Last => self
                .field(FieldReach::Here, stop)
                .await?
                .newest()
                .cloned()
                .ok_or_else(|| Refused::NoRunHere.into()),
            RunRef::Needs => self.waiting_run(stop).await,
        }
    }

    /// The run `typed` names among every run this project's store holds.
    ///
    /// Text that matches no run is handed on as the id it would be, so the
    /// operation that takes it says what it says of a run that does not
    /// exist.
    async fn typed_run(&self, typed: &str, stop: &Stop) -> Result<RunId, Error> {
        let storage = Storage::open(&self.storage_path).map_err(Failure::new)?;
        let runs = storage.list_runs().map_err(Failure::new)?;
        let ids: Vec<RunId> = runs.into_iter().map(|run| run.run_id).collect();
        match matching(typed, &ids) {
            Matched::One(run_id) => Ok(run_id),
            Matched::None => RunId::try_from(typed.to_string())
                .map_err(|invalid| Refused::InvalidId(invalid).into()),
            Matched::Several(several) => {
                // Each run as the field shows it, so a person tells them apart
                // by more than an id; one that does not read back is its id.
                let mut field = self.field(FieldReach::Machine, stop).await?;
                let candidates = several
                    .into_iter()
                    .map(|run_id| {
                        match field.rows.iter().position(|row| row.frame.run_id == run_id) {
                            Some(at) => Candidate::of(field.rows.swap_remove(at)),
                            None => Candidate::unread(run_id),
                        }
                    })
                    .collect::<Vec<_>>();
                Err(Refused::AmbiguousRun {
                    ambiguity: Ambiguity::PartOfIds {
                        typed: typed.to_string(),
                    },
                    candidates,
                }
                .into())
            }
        }
    }

    /// The one run of the repository this project is rooted in that waits
    /// on a person.
    async fn waiting_run(&self, stop: &Stop) -> Result<RunId, Error> {
        let field = self.field(FieldReach::Here, stop).await?;
        match field.needing_a_person().as_slice() {
            [] => Err(Refused::NoneNeedsYou.into()),
            [one] => Ok(one.frame.run_id.clone()),
            several => Err(Refused::AmbiguousRun {
                ambiguity: Ambiguity::SeveralNeedYou,
                candidates: several
                    .iter()
                    .map(|row| Candidate::of((*row).clone()))
                    .collect(),
            }
            .into()),
        }
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
