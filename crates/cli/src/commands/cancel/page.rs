//! What `yunta cancel` answers with: what it found and did, as the JSON
//! document carries it and as the page a person reads says it.

use surco::components::{Gist, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::counted;
use surco::words::Standing;
use yunta_core::step::NextStep;
use yunta_core::{Pid, RunId};
use yunta_render::command;

use crate::error::Outcome;
use crate::json::SCHEMA_VERSION;
use crate::output::Answer;

/// What `cancel` found and did, as a document carries it.
#[derive(serde::Serialize)]
pub(crate) struct CancelAnswer {
    schema_version: u32,
    run_id: RunId,
    #[serde(flatten)]
    state: CancelState,
}

/// How a cancel ended, and what it did to the processes that were left.
#[derive(serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum CancelState {
    /// The run was stopped already: paused, closed or broken, as
    /// `standing` calls it. `killed` process groups its engine could not
    /// stop were killed by this call; `parked` says it waits on a person.
    AlreadyStopped {
        killed: usize,
        parked: bool,
        standing: Standing,
    },
    /// No node of the run was in progress, so there was nothing to signal.
    NothingRunning,
    /// The engine recorded its terminal after the interrupt it was sent:
    /// the pause the interrupt asks for, or, `closed`, the close of a run
    /// that finished before the interrupt reached it.
    Cancelled { engine_pid: Pid, closed: bool },
    /// The engine did not stop within the shutdown timeout, so its process
    /// groups were killed.
    Escalated { engine_pid: Pid },
    /// The wait was interrupted; the interrupt stays with the engine.
    Interrupted { engine_pid: Pid },
    /// The engine had died: its orphaned process groups were killed, and the
    /// pause recorded.
    Crashed { engine_pid: Pid, killed: usize },
}

pub(super) fn answer(run_id: &RunId, state: CancelState) -> CancelAnswer {
    CancelAnswer {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.clone(),
        state,
    }
}

impl Answer for CancelAnswer {
    fn document(&self, _look: &Look) -> Doc {
        let called = self.run_id.handle();
        let Some((mark, said, gist, steps)) = self.said() else {
            // A stop the engine answers to has no line of its own: its
            // notes went to stderr while it ran.
            return Doc::new();
        };
        Doc::new()
            .with(Headline::new(mark, format!("run {called}"), said))
            .with(Gist::new(gist))
            .with(Next { steps })
    }

    fn outcome(&self) -> Outcome {
        match &self.state {
            CancelState::Escalated { .. } => Outcome::Reported,
            _ => Outcome::Success,
        }
    }
}

/// What a cancel's page says: the mark and the word of its headline, the
/// sentence under it and the commands that go on from here.
type Said = (Mark, &'static str, String, Vec<(String, &'static str)>);

impl CancelAnswer {
    /// What the page says of the run, or `None` for a stop the
    /// engine answers to itself.
    fn said(&self) -> Option<Said> {
        let stopped =
            |said, gist: String, steps| Some((Standing::Stopped.mark(), said, gist, steps));
        match &self.state {
            CancelState::AlreadyStopped {
                killed,
                parked,
                standing,
            } => Some(already_stopped(
                *standing,
                *killed,
                parked.then(|| self.closes()),
            )),
            CancelState::NothingRunning => Some((
                Mark::Absent,
                "nothing to cancel",
                "no node is in progress".to_string(),
                Vec::new(),
            )),
            CancelState::Cancelled { closed: true, .. } => Some((
                Mark::Absent,
                "nothing to cancel",
                "it closed before the interrupt reached it".to_string(),
                vec![(
                    command::line(&NextStep::status(&self.run_id)),
                    "shows how it ended",
                )],
            )),
            CancelState::Cancelled { .. } => stopped(
                Standing::Stopped.word(),
                "its engine stopped every session and recorded the pause".to_string(),
                self.goes_on(),
            ),
            CancelState::Crashed { engine_pid, killed } => stopped(
                Standing::Stopped.word(),
                format!(
                    "its engine (pid {engine_pid}) had already died, so this cancel killed {} \
                     and recorded the pause",
                    counted(*killed, "orphaned process group")
                ),
                self.goes_on(),
            ),
            CancelState::Escalated { .. } | CancelState::Interrupted { .. } => None,
        }
    }

    /// The step that closes the run for good.
    fn closes(&self) -> (String, &'static str) {
        let close = NextStep::Close {
            run: self.run_id.clone(),
        };
        (command::line(&close), "closes it for good")
    }

    /// The steps that go on from a stop: the run continued where it stopped,
    /// or closed for good.
    fn goes_on(&self) -> Vec<(String, &'static str)> {
        let resume = NextStep::Resume {
            run: self.run_id.clone(),
        };
        vec![
            (command::line(&resume), "continues it where it stopped"),
            self.closes(),
        ]
    }
}

/// What the page says of a run that had stopped before the cancel reached
/// it: the word it stands at, then what the cancel did to the process
/// groups its engine could not stop, or that there was nothing to cancel,
/// with `closes` when the run waits on a person.
fn already_stopped(
    standing: Standing,
    killed: usize,
    closes: Option<(String, &'static str)>,
) -> Said {
    let (gist, steps) = match killed {
        0 => (
            "nothing to cancel".to_string(),
            closes.into_iter().collect(),
        ),
        killed => (
            format!(
                "killed {}, which its engine could not stop",
                counted(killed, "process group")
            ),
            Vec::new(),
        ),
    };
    (standing.mark(), standing.word(), gist, steps)
}

#[cfg(test)]
mod tests {
    use super::{answer, CancelState};
    use crate::error::Outcome;
    use crate::output::Answer;
    use crate::render::draw;
    use surco::look::Look;
    use surco::words::Standing;
    use yunta_core::{Pid, RunId};

    #[test]
    fn a_cancel_that_finds_nothing_to_stop_says_so_in_its_words() {
        let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        let said = |state: CancelState| {
            let reply = answer(&run, state);
            draw(reply.document(&Look::plain()), &Look::plain())
        };
        assert_eq!(
            said(CancelState::NothingRunning),
            ". run AAAAAA ; nothing to cancel\n  no node is in progress.\n"
        );
        assert_eq!(
            said(CancelState::AlreadyStopped {
                killed: 2,
                parked: false,
                standing: Standing::Stopped,
            }),
            "# run AAAAAA ; stopped\n  killed 2 process groups, which its engine could not stop.\n"
        );
        assert_eq!(
            said(CancelState::AlreadyStopped {
                killed: 0,
                parked: true,
                standing: Standing::NeedsYou,
            }),
            "? run AAAAAA ; needs you\n  nothing to cancel.\n\nnext\n  yunta close AAAAAA   closes it for good\n"
        );
        assert_eq!(
            said(CancelState::AlreadyStopped {
                killed: 0,
                parked: false,
                standing: Standing::Finished,
            }),
            "+ run AAAAAA ; finished\n  nothing to cancel.\n"
        );
    }

    #[test]
    fn a_cancel_that_waits_on_its_engine_says_what_it_found() {
        let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        let engine = Pid::try_from(4321u32).unwrap();
        let said = |state: CancelState| {
            let reply = answer(&run, state);
            draw(reply.document(&Look::plain()), &Look::plain())
        };
        assert_eq!(
            said(CancelState::Cancelled {
                engine_pid: engine,
                closed: false
            }),
            "# run AAAAAA ; stopped\n  its engine stopped every session and recorded the pause.\n\n\
             next\n  yunta resume AAAAAA   continues it where it stopped\n  \
             yunta close AAAAAA    closes it for good\n"
        );
        // A run that finished before the interrupt reached it is no pause
        // to resume or close.
        assert_eq!(
            said(CancelState::Cancelled {
                engine_pid: engine,
                closed: true
            }),
            ". run AAAAAA ; nothing to cancel\n  it closed before the interrupt reached it.\n\n\
             next\n  yunta status AAAAAA   shows how it ended\n"
        );
        assert_eq!(
            said(CancelState::Crashed {
                engine_pid: engine,
                killed: 1
            }),
            "# run AAAAAA ; stopped\n  its engine (pid 4321) had already died, so this cancel killed 1 orphaned\n  \
             process group and recorded the pause.\n\nnext\n  yunta resume AAAAAA   continues it where it \
             stopped\n  yunta close AAAAAA    closes it for good\n"
        );
        // A stop the engine answers to draws no line of its own.
        assert_eq!(said(CancelState::Escalated { engine_pid: engine }), "");
        assert_eq!(said(CancelState::Interrupted { engine_pid: engine }), "");
        assert_eq!(
            answer(&run, CancelState::Escalated { engine_pid: engine }).outcome(),
            Outcome::Reported
        );
    }
}
