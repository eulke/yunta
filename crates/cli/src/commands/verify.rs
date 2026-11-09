//! `yunta verify <run_id>`: the two mechanical guarantees a run makes
//! about its own evidence — its log's hash chain and the objects its log
//! names — checked by the API and reported apart, either one failing a
//! failing verdict.

use surco::components::Check;
use surco::look::Look;
use yunta_core::RunId;
use yunta_render::document::VerificationDocument;
use yunta_render::page::{Page, Verified};

use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;

pub async fn verify(
    shell: &Shell,
    run_id: &RunId,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let verification = project.verification(run_id).await?;
    let answer = VerifyAnswer {
        run_id: verification.run_id.clone(),
        checks: verification.checks(),
    };
    emit(output, &answer, look)
}

/// What `yunta verify` answers: each check in the order it ran, as a
/// document for a program and as the page a person reads.
struct VerifyAnswer {
    run_id: RunId,
    checks: Vec<Check>,
}

impl serde::Serialize for VerifyAnswer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let document = VerificationDocument {
            run_id: &self.run_id,
            checks: &self.checks,
        };
        serde::Serialize::serialize(&document, serializer)
    }
}

impl Answer for VerifyAnswer {
    fn page(&self) -> impl Page + '_ {
        Verified {
            run_id: &self.run_id,
            checks: &self.checks,
        }
    }

    fn outcome(&self) -> Outcome {
        match self.checks.iter().all(Check::holds) {
            true => Outcome::Success,
            false => Outcome::Reported,
        }
    }
}
