//! The page `yunta status` prints for a run, in the order a person reads
//! it for: where the run stands, what — if anything — needs them, the
//! nodes, the evidence of what failed, the facts, and what to type next.

use std::path::Path;

use yunta_core::events::Failure;
use yunta_core::RunId;
use yunta_engine::{EngineLiveness, NodeState, RunFrame, RunPhase, RunState};

use crate::commands::advice;
use crate::render::blocks::{Block, Evidence, Fields, Headline, Next, NodeRow, NodeTable};
use crate::render::ink::{Line, Tone};
use crate::render::state::RunWord;
use crate::render::{indent, paths, prose, truncate, Look, NodeDisplay, Tokens, INDENT};

/// What the page is drawn from.
pub(super) struct Page<'a> {
    pub(super) run_id: &'a RunId,
    pub(super) frame: &'a RunFrame,
    pub(super) state: &'a RunState,
    pub(super) engine: EngineLiveness,
    pub(super) run_dir: &'a Path,
    /// Where paths are shown from: the directory this was run in, and
    /// the home `~` stands for.
    pub(super) cwd: &'a Path,
    pub(super) home: Option<&'a Path>,
}

impl Page<'_> {
    /// The word the run is called by, and the sentence that says what
    /// holds it: the verdict a reader came for, in two lines.
    pub(super) fn head(&self) -> Vec<Line> {
        let word = RunWord::observed(self.frame, self.engine);
        let mut lines = Headline {
            subject: format!("run {}", self.run_id),
            mark: word.mark(),
            said: word.word().to_string(),
        }
        .lines(&Look::stdout());
        let (tone, said) = self.holds(word);
        let look = Look::stdout();
        let room = look.width.cells().saturating_sub(INDENT.len());
        lines.push(
            Line::new()
                .plain(INDENT)
                .push(tone, truncate(&said, room, look.glyphs).trim_end()),
        );
        lines
    }

    /// What holds the run, or that nothing does.
    fn holds(&self, word: RunWord) -> (Tone, String) {
        match (word, &self.frame.phase) {
            (RunWord::Stalled, _) => (
                Tone::Caution,
                "no process is driving it: the engine that ran it is gone".to_string(),
            ),
            (_, RunPhase::Waiting { on }) => (Tone::NeedsYou, advice::parked_in_full(on)),
            (_, RunPhase::Broken { diagnostic }) => {
                (Tone::Failed, yunta_core::text::one_line(diagnostic))
            }
            (
                _,
                RunPhase::Failed {
                    failure: Some(failure),
                },
            ) => (
                Tone::Failed,
                yunta_core::text::one_line(&failure.to_string()),
            ),
            (_, RunPhase::Created | RunPhase::Running) => {
                (Tone::Muted, "nothing needs you: it is moving".to_string())
            }
            _ => (Tone::Muted, "nothing needs you".to_string()),
        }
    }

    /// Every node the frozen workflow declares, in declaration order, a
    /// group's children one step under it.
    pub(super) fn nodes(&self) -> NodeTable {
        let look = Look::stdout();
        let rows = self
            .frame
            .nodes
            .iter()
            .map(|node| {
                let display = NodeDisplay::standing(&node.state);
                let note = display
                    .modifier
                    .map(|said| prose::first_sentence(&said, look.width.cells(), look.glyphs))
                    .unwrap_or_default();
                NodeRow {
                    mark: display.word.mark(),
                    word: display.word.word(),
                    id: format!("{}{}", indent(usize::from(node.group.is_some())), node.id),
                    note,
                }
            })
            .collect();
        NodeTable { rows }
    }

    /// The last call of a run tool each node's attempt made that failed,
    /// one to a line under the table: what a node got wrong on its way
    /// is not why it stands where it does, so it is not its row's note.
    pub(super) fn calls(&self) -> Vec<Line> {
        self.frame
            .nodes
            .iter()
            .filter_map(|node| {
                let failed = self.state.nodes.get(&node.id)?.last_tool_failure.as_ref()?;
                Some(Line::new().plain(INDENT).push(
                    Tone::Muted,
                    format!(
                        "{}: last failed call of its attempt: {} ({})",
                        node.id,
                        failed.tool.name(),
                        failed.cause.as_str()
                    ),
                ))
            })
            .collect()
    }

    /// What every failed node's failure says beyond its row: the end of
    /// what a command printed and where the rest is, the problems of each
    /// document it refused, the paths it should not have touched.
    pub(super) fn evidence(&self) -> Vec<Line> {
        let look = Look::stdout();
        let mut lines = Vec::new();
        for node in &self.frame.nodes {
            let Some(NodeState::Failed { failure, .. }) = self.state.nodes.state(&node.id) else {
                continue;
            };
            let said = self.detail(failure, &look);
            if said.is_empty() {
                continue;
            }
            lines.push(
                Line::new()
                    .plain(INDENT)
                    .push(Tone::Strong, node.id.as_str()),
            );
            lines.extend(said.into_iter().map(|line| {
                let mut indented = Line::new().plain(INDENT);
                for span in line.spans() {
                    indented = indented.push(span.tone, span.text.clone());
                }
                indented
            }));
        }
        lines
    }

    /// The lines one failure has beyond the node's own row.
    fn detail(&self, failure: &Failure, look: &Look) -> Vec<Line> {
        let text = |said: String| {
            said.lines()
                .map(|line| Line::new().plain(INDENT).plain(line.to_string()))
                .collect::<Vec<_>>()
        };
        let listed = |heading: &str, paths: &[std::path::PathBuf]| {
            let mut lines = text(heading.to_string());
            lines.extend(paths.iter().map(|path| {
                Line::new()
                    .plain(indent(2))
                    .plain(path.display().to_string())
            }));
            lines
        };
        match failure {
            Failure::Exited { exited } => Evidence {
                tail: exited.tail.clone(),
                whole: exited.output.as_ref().map(|output| {
                    paths::shown(
                        &yunta_engine::ObjectStore::at(self.run_dir).path_of(output),
                        self.cwd,
                        self.home,
                    )
                }),
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
            Failure::Unchanged { .. } => text(failure.to_string()),
            Failure::ScopeViolated { .. }
            | Failure::PathsDenied { .. }
            | Failure::Message { .. }
            | Failure::ScopeRequested { .. }
            | Failure::Unset { .. } => Vec::new(),
        }
    }

    /// The facts a reader checks once they know where the run stands.
    pub(super) fn fields(&self) -> Fields {
        let tokens = self.state.total_tokens();
        let mut tasks: Vec<_> = self.state.tasks.iter().collect();
        tasks.sort_by(|a, b| a.0.cmp(b.0));
        let sep = format!(" {} ", Look::stdout().glyphs.sep());
        let tasks = tasks
            .into_iter()
            .map(|(id, record)| format!("{id} {}", super::task_status_label(record.status)))
            .collect::<Vec<_>>()
            .join(&sep);
        let suspended = self
            .state
            .run
            .suspensions()
            .summary()
            .map(|(times, slept)| {
                format!(
                    "suspended {} for {} in all — durations leave it out",
                    yunta_core::text::counted(times, "time"),
                    crate::render::duration(slept)
                )
            });
        Fields::new()
            .push_if("progress", crate::render::counter::line(self.frame))
            .push_if(
                "tokens",
                format!(
                    "{} in / {} out",
                    Tokens(tokens.input).figure(),
                    Tokens(tokens.output).figure()
                ),
            )
            .push_if("tasks", tasks)
            .push_if(
                "environment",
                self.state
                    .run
                    .environment_drift()
                    .map(|drift| drift.to_string())
                    .unwrap_or_default(),
            )
            .push_if("host", suspended.unwrap_or_default())
            .push_if(
                "unread",
                crate::commands::unknown_kinds_note(&self.frame.unknown_kinds).unwrap_or_default(),
            )
    }

    /// What a person types next, for where the run stands.
    pub(super) fn next(&self, menu: bool) -> Next {
        let handle = self.run_id.handle();
        let word = RunWord::observed(self.frame, self.engine);
        let steps = match word {
            RunWord::Stalled => vec![
                (advice::resume(handle), "continues it from its log"),
                (advice::close(handle), "closes it for good"),
            ],
            RunWord::NeedsYou if !menu => vec![
                (advice::resume(handle), "hands it back once that is settled"),
                (advice::close(handle), "closes it for good"),
            ],
            RunWord::NeedsYou => vec![(advice::close(handle), "closes it for good")],
            RunWord::Finished | RunWord::Reported => {
                vec![(advice::receipt(handle), "certifies what it did")]
            }
            RunWord::Broken => vec![(advice::verify(handle), "says where its log stops reading")],
            RunWord::Created | RunWord::Running => {
                vec![(advice::cancel(handle), "stops it and everything under it")]
            }
            RunWord::Failed | RunWord::Cancelled | RunWord::Promoted => Vec::new(),
        };
        Next { steps }
    }
}
