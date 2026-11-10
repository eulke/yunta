//! `yunta doctor`: every check the API makes of this machine and this
//! project, drawn as a page or written as a document. `--session` opens one
//! real session per binding; `--full` gives every check its own row.

#[cfg(test)]
mod tests;

use surco::look::Look;
use yunta_api::Remark;
use yunta_render::document::DoctorDocument;
use yunta_render::page::doctor::{Doctor, Finding, Readiness};
use yunta_render::page::Page;

use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;

pub async fn doctor(
    shell: &Shell,
    session: bool,
    full: bool,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let say = |remark: Remark| crate::remarks::say(remark, output, look);
    let findings = project.doctor(session, &say, &shell.stop()).await?;
    emit(
        output,
        &DoctorAnswer {
            session,
            findings,
            full,
        },
        look,
    )
}

/// What `doctor` found: what every check found in the order it ran, whether
/// `--session` opened a session per binding, and whether the page lists
/// every check on its own row.
pub(crate) struct DoctorAnswer {
    session: bool,
    findings: Vec<Finding>,
    /// Whether the page lists every check on its own row: a glance folds the
    /// ones that hold into one.
    full: bool,
}

impl serde::Serialize for DoctorAnswer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let document = DoctorDocument {
            session: self.session,
            findings: &self.findings,
        };
        serde::Serialize::serialize(&document, serializer)
    }
}

impl Answer for DoctorAnswer {
    fn page(&self) -> impl Page + '_ {
        Doctor {
            session: self.session,
            findings: &self.findings,
            full: self.full,
        }
    }

    fn outcome(&self) -> Outcome {
        match Readiness::of(&self.findings).holds() {
            true => Outcome::Success,
            false => Outcome::Reported,
        }
    }
}
