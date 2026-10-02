//! What a failure says beyond its node's row: the end of what a command
//! printed and where the rest is, the problems of each document it
//! refused, the paths it should not have touched.

use std::path::PathBuf;

use yunta_core::events::Failure;

use super::{Block, Evidence, Whole};
use crate::render::ink::Line;
use crate::render::{indent, Look, INDENT};

/// One failure's detail, as every surface that quotes it quotes it.
pub(crate) struct FailureDetail<'a> {
    pub(crate) failure: &'a Failure,
    /// Where the whole of what a command printed is: the file it is
    /// kept in, or the command that shows it.
    pub(crate) whole: Option<Whole>,
}

impl Block for FailureDetail<'_> {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let text = |said: String| {
            said.lines()
                .map(|line| Line::new().plain(INDENT).plain(line.to_string()))
                .collect::<Vec<_>>()
        };
        let listed = |heading: &str, paths: &[PathBuf]| {
            let mut lines = text(heading.to_string());
            lines.extend(paths.iter().map(|path| {
                Line::new()
                    .plain(indent(2))
                    .plain(path.display().to_string())
            }));
            lines
        };
        match self.failure {
            Failure::Exited { exited } => Evidence {
                tail: exited.tail.clone(),
                whole: self.whole.clone(),
            }
            .lines(look),
            Failure::SessionDied { died } => {
                let tail: Vec<String> = died
                    .exit
                    .iter()
                    .flat_map(|exit| exit.stderr_tail.clone())
                    .collect();
                match tail.is_empty() {
                    true => Vec::new(),
                    false => Evidence { tail, whole: None }.lines(look),
                }
            }
            Failure::Artifacts { artifacts } => artifacts
                .iter()
                .flat_map(|artifact| text(artifact.to_string()))
                .collect(),
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
            Failure::ScopeViolated { .. }
            | Failure::PathsDenied { .. }
            | Failure::Message { .. }
            | Failure::ScopeRequested { .. }
            | Failure::Unset { .. } => Vec::new(),
        }
    }
}
