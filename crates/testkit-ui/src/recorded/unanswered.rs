//! See [`super`]. Each kind of work a person waits on to act, and of act a person takes, that
//! the walk's bound has room for, answered refused and failed below Yunta beside what it came
//! to where it was performed.

use yunta_core::NodeId;
use yunta_render::refusal::Refused;
use yunta_ui::{Operation, Outcome};

use super::Recordings;
use crate::home::Home;

/// What failed below Yunta where the walk answers a piece of work as failed, in the words a
/// failure is handed back in.
const UNANSWERED: &str = "the disk is full: no space left on device";

impl Recordings {
    /// Each kind of act a person takes, and of work a person waits on to take one, that the
    /// walk's bound has room for, answered as refused and as failed below Yunta: answering a
    /// run's round, resuming and closing a run, a setup's write, as `write` asked it, its
    /// detection, and a pack's install, as `install` asked it. A close's come inside what
    /// closing came to, as the performer hands them. Deciding, stopping, the workflows read,
    /// doctor's checks and collecting what runs left are not: each alone adds from three
    /// thousand states to more than twenty thousand, past what the bound leaves the walk.
    pub(super) fn unanswered(&mut self, home: &Home, write: Operation, install: Operation) {
        let run = home.parked.clone();
        let kinds = [
            Operation::Answer {
                run: run.clone(),
                node: NodeId::from("ask"),
                answers: Vec::new(),
            },
            Operation::Resume { run: run.clone() },
            Operation::Close { runs: vec![run] },
            write,
            Operation::DetectProject,
            install,
        ];
        let refusal = || Refused::NoRunHere;
        let failure = || UNANSWERED.to_string();
        for operation in kinds {
            let (refused, failed) = match operation {
                Operation::Close { .. } => (
                    Outcome::Closed {
                        closed: Vec::new(),
                        refused: vec![refusal()],
                        failed: None,
                    },
                    Outcome::Closed {
                        closed: Vec::new(),
                        refused: Vec::new(),
                        failed: Some(failure()),
                    },
                ),
                _ => (Outcome::Refused(refusal()), Outcome::Failed(failure())),
            };
            self.kept((operation.clone(), Vec::new(), refused));
            self.kept((operation, Vec::new(), failed));
        }
    }
}
