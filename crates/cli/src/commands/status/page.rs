//! The page `yunta status` prints for a run, in the order a person reads
//! it for: where the run stands, what — if anything — needs them, the
//! nodes, the evidence of what failed, the facts, and what to type next.

use std::path::Path;

use yunta_core::events::Failure;
use yunta_core::RunId;
use yunta_engine::{EngineLiveness, NodeState, RunFrame, RunPhase, RunState};

use crate::commands::advice;
use crate::render::blocks::{
    Block, FailureDetail, Fields, Headline, Next, NodeRow, NodeTable, Whole,
};
use crate::render::ink::{Line, Tone};
use crate::render::state::RunWord;
use crate::render::{indent, prose, truncate, Look, NodeDisplay, Tokens, INDENT};

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
            (RunWord::Stalled, _) => (Tone::Caution, advice::STALLED.to_string()),
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
            let mut said = self.claim_cut_short(failure, &look);
            said.extend(self.detail(failure, &look));
            if said.is_empty() {
                continue;
            }
            lines.push(
                Line::new()
                    .plain(INDENT)
                    .push(Tone::Strong, node.id.as_str()),
            );
            lines.extend(said.into_iter().map(|line| line.under(INDENT)));
        }
        lines
    }

    /// A failure's claim in full, when the node's row has no room for
    /// it: the row cuts it, and the one sentence that says why a node
    /// failed is never only seen cut.
    fn claim_cut_short(&self, failure: &Failure, look: &Look) -> Vec<Line> {
        // A claim that sums up the lines under it — each refused
        // document's heading, how many files fell outside the scope — is
        // said in full by them.
        if matches!(
            failure,
            Failure::Artifacts { .. } | Failure::ScopeViolated { .. }
        ) {
            return Vec::new();
        }
        let claim = failure.headline();
        // What a row leaves its note: the indent, the mark, the word and
        // an id column as wide as the longest id.
        let ids = crate::render::id_column(self.frame.nodes.iter().map(|node| node.id.as_str()));
        let room = look
            .width
            .cells()
            .saturating_sub(INDENT.len() + 2 + crate::render::STATE_WIDTH + 1 + ids + 2);
        if crate::render::cell_width(&claim) <= room {
            return Vec::new();
        }
        let under = INDENT.len() * 2;
        crate::render::wrap(&claim, look.width.cells().saturating_sub(under))
            .into_iter()
            .map(|line| Line::new().plain(INDENT).plain(line))
            .collect()
    }

    /// The lines one failure has beyond the node's own row, with where
    /// the whole of what a command printed is kept.
    fn detail(&self, failure: &Failure, look: &Look) -> Vec<Line> {
        let whole = failure.output().map(|output| {
            Whole::file(
                yunta_engine::ObjectStore::at(self.run_dir).path_of(output),
                self.cwd,
                self.home,
            )
        });
        FailureDetail { failure, whole }.lines(look)
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
            .push_if(
                "progress",
                crate::render::counter::line(self.frame, Look::stdout().glyphs),
            )
            .push_if("tokens", spent(tokens))
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
        let word = RunWord::observed(self.frame, self.engine);
        Next {
            steps: advice::after(word, self.run_id.handle(), menu),
        }
    }
}

/// What a run spent, in and out — nothing for a run that spent none.
fn spent(tokens: yunta_core::events::TokenUsage) -> String {
    match tokens.total() {
        0 => String::new(),
        _ => format!(
            "{} in / {} out",
            Tokens(tokens.input).figure(),
            Tokens(tokens.output).figure()
        ),
    }
}
