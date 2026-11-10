//! The verdict on a run's evidence, as a person reads it: whether it is
//! intact, and each check in the order it ran — the event chain, then the
//! objects its log names — said from what reading them found.

use std::path::PathBuf;

use surco::components::{Check, Found, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::events::{FindingCause, ObjectProblem};
use yunta_core::receipt::EventChainStatus;
use yunta_core::RunId;

use super::checklist::{checklist, tallied};
use super::Page;

/// What checking a run's evidence found: its log's hash chain, recomputed
/// from the bytes as persisted, and the objects its log names, each read back
/// and hashed against its own name. The two are independent, so each is a
/// check of its own, and either one failing fails the verdict.
pub struct Verification {
    pub run_id: RunId,
    pub chain: EventChainStatus,
    pub objects: ObjectsRead,
}

/// What reading a run's objects back found.
pub enum ObjectsRead {
    /// The run has no directory under `runs_root`, nor under the default
    /// state root: the log outlives the directory, so the bytes it names are
    /// not here to read.
    NoDirectory { runs_root: PathBuf },
    /// The log does not read back as events, so nothing names the objects:
    /// the store's own words.
    UnreadableLog(String),
    /// Each object the log names, read back.
    Read(ObjectsChecked),
}

/// Each object a run's log names, read back: how many are the bytes the run
/// accepted, each that is not, and how many a log older than the object store
/// names with no object to check.
pub struct ObjectsChecked {
    pub verified: usize,
    pub faults: Vec<ObjectFault>,
    pub unverifiable: usize,
}

/// One artifact whose bytes are not the bytes the run accepted: the path of
/// its view, which names it, and what the store answered.
pub struct ObjectFault {
    pub artifact: String,
    pub cause: FaultCause,
}

/// What the store answered for an artifact that is not the bytes the run
/// accepted: the object's own problem, or the step of the store's that failed.
pub enum FaultCause {
    Object(ObjectProblem),
    Failed { step: String },
}

impl Verification {
    /// Each check as a person and a program read it, in the order they ran:
    /// the chain, then the objects.
    pub fn checks(&self) -> Vec<Check> {
        let mut checks = vec![self.chain_check()];
        match &self.objects {
            ObjectsRead::NoDirectory { runs_root } => checks.push(Check::new(
                Found::Caution,
                "objects",
                format!(
                    "not checked: no run directory under {} (or the default state root), so \
                     the bytes its log names are not here to read",
                    runs_root.display()
                ),
            )),
            ObjectsRead::UnreadableLog(error) => checks.push(Check::new(
                Found::Problem,
                "objects",
                format!(
                    "not checked: the log does not read back as events, so nothing names \
                     them: {error}"
                ),
            )),
            ObjectsRead::Read(read) => checks.extend(read.checks()),
        }
        checks
    }

    /// The chain's check: intact with how many events it verified, or broken
    /// at the seq it breaks at, from where the log can no longer be trusted.
    fn chain_check(&self) -> Check {
        match &self.chain {
            EventChainStatus::Intact { events } => Check::new(
                Found::Holds,
                "chain",
                format!(
                    "intact: {} verified",
                    surco::text::counted(*events, "event")
                ),
            ),
            EventChainStatus::Broken { seq, detail } => Check::new(
                Found::Problem,
                "chain",
                format!("broken at seq {seq}: {detail}"),
            ),
        }
    }
}

impl ObjectsChecked {
    /// The objects' check, then one for each artifact that is not the bytes
    /// the run accepted, then what was left unchecked.
    fn checks(&self) -> Vec<Check> {
        let mut checks = Vec::new();
        match self.faults.is_empty() {
            true => checks.push(Check::new(
                Found::Holds,
                "objects",
                format!(
                    "intact: {} verified",
                    surco::text::counted(self.verified, "artifact")
                ),
            )),
            false => {
                let named = self.verified + self.faults.len();
                checks.push(Check::new(
                    Found::Problem,
                    "objects",
                    format!(
                        "broken: {} of {} are not the bytes the run accepted",
                        self.faults.len(),
                        surco::text::counted(named, "artifact")
                    ),
                ));
                checks.extend(self.faults.iter().map(|fault| {
                    Check::new(Found::Problem, fault.artifact.clone(), fault.cause.said())
                }));
            }
        }
        if self.unverifiable > 0 {
            let cause = FindingCause::ArtifactsUnverified {
                unchecked: self.unverifiable,
                verified: self.verified,
            };
            checks.push(Check::new(
                Found::Caution,
                "objects",
                crate::found::detail(&cause),
            ));
        }
        checks
    }
}

impl FaultCause {
    /// What an artifact that is not the bytes the run accepted says of
    /// itself: the object's problem in the words a reader sees, or the
    /// store's own failed step.
    fn said(&self) -> String {
        match self {
            FaultCause::Object(problem) => crate::failure::object(problem),
            FaultCause::Failed { step } => format!("failed to {step}"),
        }
    }
}

/// The verdict on a run's evidence, as a person reads it: whether it is
/// intact, and each check in the order it ran.
pub struct Verified<'a> {
    pub run_id: &'a RunId,
    pub checks: &'a [Check],
}

impl Page for Verified<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let integrity = Integrity::of(self.checks);
        let mark = match integrity.holds() {
            true => Mark::Done,
            false => Mark::Failed,
        };
        let headline = tallied(
            Headline::new(
                mark,
                format!("run {}", self.run_id.handle()),
                integrity.word(),
            ),
            self.checks,
        );
        Doc::new().with(headline).with(checklist(self.checks))
    }
}

/// Whether a run's evidence is intact, as every check of it finds: the
/// verdict its page and its document both say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Integrity {
    /// Every check holds.
    Intact,
    /// A check does not hold.
    Broken,
}

impl Integrity {
    /// The integrity `checks` find: intact only when every one holds.
    pub(crate) fn of(checks: &[Check]) -> Integrity {
        match checks.iter().all(Check::holds) {
            true => Integrity::Intact,
            false => Integrity::Broken,
        }
    }

    /// Whether every check holds.
    pub(crate) fn holds(self) -> bool {
        self == Integrity::Intact
    }

    /// The word the verdict says.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Integrity::Intact => "intact",
            Integrity::Broken => "broken",
        }
    }
}

#[cfg(test)]
mod tests {
    use surco::components::Found;

    use super::*;

    /// Each check a verification makes, as what it found, of what, and what
    /// it says.
    fn said(verification: &Verification) -> Vec<(Found, String, String)> {
        verification
            .checks()
            .into_iter()
            .map(|check| (check.found, check.subject, check.said))
            .collect()
    }

    fn intact_chain(objects: ObjectsRead) -> Verification {
        Verification {
            run_id: RunId::from_static("01K3W48MFW7H0ZZA5PZ07E5PH4"),
            chain: EventChainStatus::Intact { events: 3 },
            objects,
        }
    }

    /// A run whose directory is gone still answers for its log: its objects
    /// are not checked, and say why, as a caution beside the chain.
    #[test]
    fn objects_with_no_directory_are_a_caution_beside_the_chain() {
        let verification = intact_chain(ObjectsRead::NoDirectory {
            runs_root: PathBuf::from("/runs"),
        });
        assert_eq!(
            said(&verification),
            vec![
                (
                    Found::Holds,
                    "chain".to_string(),
                    "intact: 3 events verified".to_string()
                ),
                (
                    Found::Caution,
                    "objects".to_string(),
                    "not checked: no run directory under /runs (or the default state root), \
                     so the bytes its log names are not here to read"
                        .to_string()
                ),
            ]
        );
    }

    /// Each artifact that is not the bytes the run accepted is a problem of
    /// its own, under the count of them among every artifact the log names.
    #[test]
    fn each_artifact_not_accepted_is_a_problem_of_its_own() {
        let verification = intact_chain(ObjectsRead::Read(ObjectsChecked {
            verified: 2,
            faults: vec![ObjectFault {
                artifact: "artifacts/plan.md".to_string(),
                cause: FaultCause::Failed {
                    step: "read objects/ab".to_string(),
                },
            }],
            unverifiable: 0,
        }));
        let checks = said(&verification);
        assert_eq!(
            checks[1..],
            [
                (
                    Found::Problem,
                    "objects".to_string(),
                    "broken: 1 of 3 artifacts are not the bytes the run accepted".to_string()
                ),
                (
                    Found::Problem,
                    "artifacts/plan.md".to_string(),
                    "failed to read objects/ab".to_string()
                ),
            ]
        );
    }
}
