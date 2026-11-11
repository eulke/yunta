//! `yunta list --runs`, and `yunta` alone in a repository: the field —
//! this repository's runs, or every run on the machine — read by the API
//! and drawn as render's field page.

use std::path::PathBuf;

use surco::doc::Depth;
use surco::look::Look;
use yunta_api::{FieldReach, FieldReading};
use yunta_render::document::FieldDocument;
use yunta_render::page::{Field, Page};

use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;

/// The field `reach` asks for, as deep as `depth` asks.
pub async fn list_runs(
    shell: &Shell,
    reach: FieldReach,
    depth: Depth,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let reading = project.field(reach, &shell.stop()).await?;
    let answer = ListRunsAnswer {
        reading,
        storage: project.storage_path,
        depth,
    };
    emit(output, &answer, look)
}

/// What `yunta list --runs` answers: the field the API read, and where this
/// project keeps its runs, which a field with none names.
struct ListRunsAnswer {
    reading: FieldReading,
    storage: PathBuf,
    depth: Depth,
}

impl serde::Serialize for ListRunsAnswer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let document = FieldDocument {
            storage: &self.storage,
            rows: &self.reading.rows,
            unreadable: &self.reading.unreadable,
            elsewhere: self.reading.elsewhere,
        };
        serde::Serialize::serialize(&document, serializer)
    }
}

impl Answer for ListRunsAnswer {
    fn page(&self) -> impl Page + '_ {
        Field {
            rows: &self.reading.rows,
            unreadable: &self.reading.unreadable,
            elsewhere: self.reading.elsewhere,
            here: self.reading.here,
            storage: &self.storage,
            depth: self.depth,
            pointed: None,
            marked: &[],
        }
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}
