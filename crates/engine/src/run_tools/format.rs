//! What a session writes, formatted the moment it writes it.
//!
//! A CLI's hook tells the engine after every write. Each file the session
//! may write that a `format:` rule covers is handed to that rule's command,
//! the way the project formats it, with every such file of the write as an
//! argument. The formatters of one write share one time bound, and a
//! command still running when it ends is stopped. The engine compares the
//! files on either side of the run, records `write_formatted`, and tells the
//! session only when something changed — so it reads the file again ahead of
//! its next edit — or the command failed or was stopped. A format a session
//! never has to think about leaves nothing for a lint to find later.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use yunta_core::events::{EventPayload, SessionEvent, WriteFormattedPayload};
use yunta_core::fence::Verdict;
use yunta_core::rail::answer::{FormatAnswer, FormatNote};
use yunta_core::rail::hook::RailFailureCause;
use yunta_core::rail::window;
use yunta_core::rail::RailEvent;
use yunta_core::{ContentHash, SessionId};

use super::host::Formatter;
use super::session::SessionTools;
use crate::artifacts::store::ObjectStore;
use crate::process::{spawn_governed, CommandOutput, GovernedCommand, Outcome};

/// What one run of a formatter left: what the log keeps, and what the session reads.
struct Ran {
    exit_code: i32,
    timed_out: bool,
    duration_ms: u64,
    output: Option<ContentHash>,
    printed: Vec<String>,
}

/// What a formatter's command left when it ended: the bytes its output is kept from.
struct Spawned {
    exit_code: i32,
    timed_out: bool,
    bytes: Vec<u8>,
    lines: Vec<String>,
}

impl SessionTools {
    /// What the session is told after it wrote `paths`. The formatters of
    /// one write share one bound, which starts here.
    pub(super) async fn after_write(&self, session: &SessionId, paths: &[PathBuf]) -> FormatAnswer {
        if self.host.formatters.is_empty() {
            return FormatAnswer::Proceed;
        }
        let deadline = Instant::now() + Duration::from_millis(window::FORMAT_MS);
        let written = self.formattable(paths).await;
        let mut notes = Vec::new();
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
            notes.extend(self.formatted(session, formatter, covered, deadline).await);
        }
        match notes.is_empty() {
            true => FormatAnswer::Proceed,
            false => FormatAnswer::Tell { notes },
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
        deadline: Instant,
    ) -> Option<FormatNote> {
        let before = digests(&self.cwd, &paths).await;
        let run = self
            .run_formatter(session, formatter, &paths, deadline)
            .await;
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
            paths,
            changed: changed.clone(),
            exit_code: run.exit_code,
            duration_ms: run.duration_ms,
            output: run.output.clone(),
            timed_out: run.timed_out,
        }));
        if let Err(error) = self.append(payload).await {
            self.rails.left_unrecorded(error);
        }
        told(formatter, changed, &run)
    }

    /// `formatter`'s command over `paths`, stopped at `deadline`. What it
    /// printed is kept redacted, and the session reads that same version.
    async fn run_formatter(
        &self,
        session: &SessionId,
        formatter: &Formatter,
        paths: &[PathBuf],
        deadline: Instant,
    ) -> Ran {
        let started = Instant::now();
        let spawned = self
            .spawned(
                session,
                formatter,
                paths,
                deadline.saturating_duration_since(started),
            )
            .await;
        let duration_ms = yunta_core::units::whole_millis(started.elapsed());
        // The output is read back by the hash its event names, which verifies it.
        // best-effort: a store that fails leaves the event without output.
        let output = ObjectStore::at(&self.host.run_dir)
            .put_redacted(&spawned.bytes, &self.host.redactor)
            .await
            .ok();
        let printed: Vec<String> = spawned
            .lines
            .iter()
            .map(|line| self.host.redactor.text(line))
            .collect();
        Ran {
            exit_code: spawned.exit_code,
            timed_out: spawned.timed_out,
            duration_ms,
            output,
            printed,
        }
    }

    /// `formatter`'s command over `paths`, given `limit` to finish in: what
    /// it left when it ended, or was stopped.
    async fn spawned(
        &self,
        session: &SessionId,
        formatter: &Formatter,
        paths: &[PathBuf],
        limit: Duration,
    ) -> Spawned {
        let script = format!("{} \"$@\"", formatter.command);
        let command = paths
            .iter()
            .fold(
                GovernedCommand::shell(&self.cwd, &script).arg("yunta-format"),
                |command, path| command.arg(path.display().to_string()),
            )
            .timeout(limit);
        match spawn_governed(command, self.host.supervision(&self.stop)).await {
            Ok(outcome) => {
                let printed = CommandOutput::of(&outcome);
                Spawned {
                    exit_code: outcome.exit_code(),
                    timed_out: matches!(outcome, Outcome::TimedOut { .. }),
                    bytes: printed.bytes().to_vec(),
                    lines: printed.tail(),
                }
            }
            Err(error) => {
                self.failed(
                    Some(session),
                    Some(RailEvent::AfterWrite),
                    RailFailureCause::FormatterFailed,
                )
                .await;
                let text = error.to_string();
                Spawned {
                    exit_code: -1,
                    timed_out: false,
                    bytes: text.clone().into_bytes(),
                    lines: vec![text],
                }
            }
        }
    }
}

/// What the session reads of one formatter's run: nothing when it
/// changed nothing and exited 0.
fn told(formatter: &Formatter, changed: Vec<PathBuf>, run: &Ran) -> Option<FormatNote> {
    let name = formatter.run.clone();
    match (run.timed_out, run.exit_code, changed.is_empty()) {
        (true, _, _) => Some(FormatNote::TimedOut {
            run: name,
            after_ms: run.duration_ms,
        }),
        (false, 0, true) => None,
        (false, 0, false) => Some(FormatNote::Changed {
            run: name,
            paths: changed,
        }),
        (false, exit_code, _) => Some(FormatNote::Failed {
            run: name,
            exit_code,
            printed: run.printed.clone(),
        }),
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
