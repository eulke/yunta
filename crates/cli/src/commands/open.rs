//! `yunta open`: a run's page as a browser reads it, in the run's own folder.
//! A person at a terminal gets it opened; a script gets where it is.

use std::io::IsTerminal;
use std::path::Path;
use std::process::{Command, Stdio};

use surco::components::{Fields, Note};
use surco::doc::Doc;
use surco::look::Look;
use surco::surface::{Html, Surface};
use yunta_core::RunId;

use super::status::page_for_a_browser;
use crate::context::Context;
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};

/// The file a run's page is written to, in the run's folder.
const PAGE: &str = "page.html";

/// Writes the run's page into its folder, and opens it when a person at a
/// terminal asks for a page to read; any other reader only hears where it is.
pub async fn open(run_id: &RunId, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let (doc, run_dir) = page_for_a_browser(&ctx, run_id).await?;
    let path = run_dir.join(PAGE);
    super::receipt::save(&path, &Html.draw(&doc))?;
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
    fn document(&self, _look: &Look) -> Doc {
        let doc = Doc::new().with(Fields::new().push_literal("page", self.page.clone()));
        match self.opened {
            Some(false) => doc.with(Note(
                "nothing here could open it: open that file in a browser.".to_string(),
            )),
            _ => doc,
        }
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}
