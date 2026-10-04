//! What a failure says beyond its node's row: the end of what a command
//! printed and where the rest is, the problems of each document it
//! refused, the paths it should not have touched.

use std::path::PathBuf;

use yunta_core::events::Failure;

use super::{Drawn, Evidence, Whole};
use crate::ink::Line;
use crate::{indent, Look, INDENT};

/// One failure's detail, as every surface that quotes it quotes it.
pub struct FailureDetail<'a> {
    pub failure: &'a Failure,
    /// Where the whole of what a command printed is: the file it is
    /// kept in, or the command that shows it.
    pub whole: Option<Whole>,
}

/// What a failure's detail says, whatever medium draws it.
pub enum FailureSays {
    /// The end of what a command printed, and where the rest is.
    Evidence(Evidence),
    /// Lines that say it whole.
    Text(Vec<String>),
    /// A heading, and each path it names.
    Listed {
        heading: String,
        paths: Vec<PathBuf>,
    },
    /// Nothing beyond the node's own row.
    Nothing,
}

impl FailureDetail<'_> {
    /// What this failure says beyond its node's row.
    pub fn says(&self) -> FailureSays {
        let text = |said: String| FailureSays::Text(said.lines().map(str::to_string).collect());
        let listed = |heading: &str, paths: &[PathBuf]| FailureSays::Listed {
            heading: heading.to_string(),
            paths: paths.to_vec(),
        };
        match self.failure {
            Failure::Exited { exited } => FailureSays::Evidence(Evidence {
                tail: exited.tail.clone(),
                whole: self.whole.clone(),
            }),
            Failure::SessionDied { died } => {
                let tail: Vec<String> = died
                    .exit
                    .iter()
                    .flat_map(|exit| exit.stderr_tail.clone())
                    .collect();
                match tail.is_empty() {
                    true => FailureSays::Nothing,
                    false => FailureSays::Evidence(Evidence { tail, whole: None }),
                }
            }
            Failure::Artifacts { artifacts } => FailureSays::Text(
                artifacts
                    .iter()
                    .flat_map(|artifact| {
                        artifact
                            .to_string()
                            .lines()
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .collect(),
            ),
            Failure::ScopeViolated { outside_scope } if outside_scope.len() > 1 => {
                listed("outside the declared globs:", outside_scope)
            }
            Failure::PathsDenied { denied_paths } if denied_paths.len() > 1 => listed(
                "denied to every session of the run — by the project (permissions.paths.deny), \
                 or as a test a person approved:",
                denied_paths,
            ),
            Failure::Message { outcome } if outcome.contains('\n') => text(outcome.clone()),
            Failure::Unchanged { .. } => text(self.failure.to_string()),
            Failure::ScopeOwed { .. } => FailureSays::Text(self.failure.detail()),
            Failure::ScopeViolated { .. }
            | Failure::PathsDenied { .. }
            | Failure::Message { .. }
            | Failure::ScopeRequested { .. }
            | Failure::Unset { .. } => FailureSays::Nothing,
        }
    }
}

impl Drawn for FailureDetail<'_> {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let text = |said: &str| Line::new().plain(INDENT).plain(said.to_string());
        match self.says() {
            FailureSays::Evidence(evidence) => evidence.lines(look),
            FailureSays::Text(lines) => lines.iter().map(|said| text(said)).collect(),
            FailureSays::Listed { heading, paths } => std::iter::once(text(&heading))
                .chain(paths.iter().map(|path| {
                    Line::new()
                        .plain(indent(2))
                        .plain(path.display().to_string())
                }))
                .collect(),
            FailureSays::Nothing => Vec::new(),
        }
    }
}
