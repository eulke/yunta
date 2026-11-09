//! `yunta cancel <run_id>`: interrupt, then kill. The API stops the run's
//! whole process tree rather than leaving anything orphaned; this command
//! says what the cancel says while it works, and answers how it ended.

use surco::look::Look;
use yunta_api::Remark;
use yunta_core::RunId;
use yunta_render::document::CancelDocument;
use yunta_render::page::{CancelState, Cancelled, Page};

use crate::context::Context;
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;

pub async fn cancel(
    shell: &Shell,
    run_id: &RunId,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::of(shell)?;
    let open = ctx.project.run(run_id).await?;
    let say = |remark: Remark| crate::remarks::say(remark, output, look);
    let state = open.cancel(&say, &ctx.stop()).await?;
    let answer = CancelAnswer {
        run_id: run_id.clone(),
        state,
    };
    emit(output, &answer, look)
}

/// What `cancel` found and did: the run, and how the cancel ended.
pub(crate) struct CancelAnswer {
    run_id: RunId,
    state: CancelState,
}

impl serde::Serialize for CancelAnswer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let document = CancelDocument {
            run_id: &self.run_id,
            state: &self.state,
        };
        serde::Serialize::serialize(&document, serializer)
    }
}

impl Answer for CancelAnswer {
    fn page(&self) -> impl Page + '_ {
        Cancelled {
            run_id: &self.run_id,
            state: &self.state,
        }
    }

    fn outcome(&self) -> Outcome {
        match &self.state {
            CancelState::Escalated { .. } => Outcome::Reported,
            _ => Outcome::Success,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CancelAnswer;
    use crate::output::{Answer, Outcome};
    use yunta_core::{Pid, RunId};
    use yunta_render::page::CancelState;

    /// A cancel that had to kill its engine's process groups reports it in
    /// its exit: the run was not stopped the way it asks to be.
    #[test]
    fn a_cancel_that_kills_what_its_engine_left_reports_it() {
        let run_id = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        let engine = Pid::try_from(4321u32).unwrap();
        let reply = CancelAnswer {
            run_id,
            state: CancelState::Escalated { engine_pid: engine },
        };
        assert_eq!(reply.outcome(), Outcome::Reported);
    }
}
