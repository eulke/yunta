//! `yunta open`: a run's page as a browser reads it, which the API writes in the
//! run's own folder. A person at a terminal gets it opened; a script gets where it
//! is.

use std::io::IsTerminal;
use std::path::Path;
use std::process::{Command, Stdio};

use surco::look::Look;
use yunta_core::RunId;
use yunta_render::page::{Opened, Page};

use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;

/// Writes the run's page into its folder, and opens it when a person at a
/// terminal asks for a page to read; any other reader only hears where it is.
pub async fn open(
    shell: &Shell,
    run_id: &RunId,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let remarks = |remark| crate::remarks::say(remark, output, look);
    let path = project
        .run(run_id)
        .await?
        .page_for_a_browser(&remarks)
        .await?;
    let opened = match output {
        Mode::Human if std::io::stdout().is_terminal() => Some(browser(&path).is_ok()),
        _ => None,
    };
    emit(
        output,
        &OpenAnswer {
            page: path.display().to_string(),
            opened,
        },
        look,
    )
}

/// Asks the system's own opener for `page`. The opener hands the page to the
/// browser and is gone, so nothing waits on it.
fn browser(page: &Path) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(page)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

/// Where the page is, and, for a person at a terminal, whether it opened.
#[derive(serde::Serialize)]
struct OpenAnswer {
    page: String,
    #[serde(skip)]
    opened: Option<bool>,
}

impl Answer for OpenAnswer {
    fn page(&self) -> impl Page + '_ {
        Opened {
            page: &self.page,
            opened: self.opened,
        }
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}
