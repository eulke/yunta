//! What a session writes, formatted the moment it writes it.
//!
//! A CLI's hook tells the engine after every write. Each file the session
//! may write that a `format:` rule covers is handed to that rule's command,
//! the way the project formats it, with every such file of the write as an
//! argument. The engine compares the files before and after, records
//! `write_formatted`, and tells the session only when something changed —
//! so it reads the file again before its next edit — or the command
//! failed. A format a session never has to think about leaves nothing for
//! a lint to find later.

use std::path::{Path, PathBuf};

use yunta_core::events::{EventPayload, SessionEvent, WriteFormattedPayload};
use yunta_core::fence::Verdict;
use yunta_core::rail::RailAnswer;
use yunta_core::{ContentHash, SessionId};

use super::host::Formatter;
use super::session::SessionTools;
use crate::process::{spawn_governed, CommandOutput, GovernedCommand};

impl SessionTools {
    /// What the session is told after it wrote `paths`.
    pub(super) async fn after_write(&self, session: &SessionId, paths: &[PathBuf]) -> RailAnswer {
        let written = self.formattable(paths).await;
        let mut told = Vec::new();
        let mut taken: Vec<&PathBuf> = Vec::new();
        for formatter in &self.host.formatters {
            let covered: Vec<PathBuf> = written
                .iter()
                .filter(|path| !taken.contains(path) && formatter.covers(path))
                .cloned()
                .collect();
            if covered.is_empty() {
                continue;
            }
            taken.extend(written.iter().filter(|path| covered.contains(path)));
            told.extend(self.formatted(session, formatter, covered).await);
        }
        match told.is_empty() {
            true => RailAnswer::Proceed,
            false => RailAnswer::Tell {
                context: told.join("\n\n"),
            },
        }
    }

    /// Of `paths`, the files this session may write that exist now, each
    /// once and relative to the worktree.
    async fn formattable(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        let Some((fence, confinement)) = self.rails.fence_of() else {
            return Vec::new();
        };
        let mut files = Vec::new();
        for path in paths {
            if !matches!(fence.judge(&self.cwd, path, confinement), Verdict::Allowed) {
                continue;
            }
            let absolute = self.cwd.join(path);
            let Ok(relative) = absolute.strip_prefix(&self.cwd).map(Path::to_path_buf) else {
                continue;
            };
            let is_file = tokio::fs::metadata(&absolute)
                .await
                .is_ok_and(|meta| meta.is_file());
            if is_file && !files.contains(&relative) {
                files.push(relative);
            }
        }
        files
    }

    /// Runs `formatter` on `paths` and records what it did; what the
    /// session is told of it, when anything.
    async fn formatted(
        &self,
        session: &SessionId,
        formatter: &Formatter,
        paths: Vec<PathBuf>,
    ) -> Option<String> {
        let before = digests(&self.cwd, &paths).await;
        let started = std::time::Instant::now();
        let script = format!("{} \"$@\"", formatter.command);
        let command = paths.iter().fold(
            GovernedCommand::shell(&self.cwd, &script).arg("yunta-format"),
            |command, path| command.arg(path.display().to_string()),
        );
        let outcome = spawn_governed(command, self.host.supervision(&self.stop)).await;
        let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let (exit_code, printed) = match outcome {
            Ok(outcome) => {
                let printed = CommandOutput::of(&outcome);
                (crate::task_cycle::exit_code_of(outcome), printed.tail())
            }
            Err(error) => (-1, vec![error.to_string()]),
        };
        let after = digests(&self.cwd, &paths).await;
        let changed: Vec<PathBuf> = paths
            .iter()
            .zip(before.iter().zip(&after))
            .filter(|(_, (before, after))| before != after)
            .map(|(path, _)| path.clone())
            .collect();
        let payload = EventPayload::Session(SessionEvent::WriteFormatted(WriteFormattedPayload {
            session_id: session.clone(),
            run: formatter.run.clone(),
            paths: paths.clone(),
            changed: changed.clone(),
            exit_code,
            duration_ms,
        }));
        if let Err(error) = self.append(payload).await {
            tracing::debug!(%error, "a format went unrecorded");
        }
        told(formatter, &changed, exit_code, &printed)
    }
}

/// What the session reads of one formatter's run: nothing when it
/// changed nothing and exited 0.
fn told(
    formatter: &Formatter,
    changed: &[PathBuf],
    exit_code: i32,
    printed: &[String],
) -> Option<String> {
    let names = |paths: &[PathBuf]| {
        paths
            .iter()
            .map(|path| format!("`{}`", path.display()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match (exit_code, changed.is_empty()) {
        (0, true) => None,
        (0, false) => Some(format!(
            "yunta: `{}` formatted {} after your write; read it again before you edit it.",
            formatter.run,
            names(changed)
        )),
        (code, _) => Some(format!(
            "yunta: `{}` could not format what you wrote (exit {code}):\n{}",
            formatter.run,
            printed.join("\n")
        )),
    }
}

/// Each file's content digest, `None` for one that cannot be read.
async fn digests(cwd: &Path, paths: &[PathBuf]) -> Vec<Option<ContentHash>> {
    let mut digests = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = tokio::fs::read(cwd.join(path)).await.ok();
        digests.push(bytes.map(|bytes| yunta_core::sha256_hex(&bytes)));
    }
    digests
}
