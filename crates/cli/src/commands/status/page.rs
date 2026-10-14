//! The sections `yunta status` shows for a run that may be moving: its
//! nodes, the evidence of what failed, the facts a reader checks, and what
//! to type next. The page that holds them is composed in [`crate::run_page`].

use yunta_core::events::Failure;
use yunta_engine::{NodeState, RunState};

use crate::commands::advice;
use crate::render::blocks::{Drawn, FailureDetail, Fields, Next, NodeRow, NodeTable, Whole};
use crate::render::ink::{Line, Tone};
use crate::render::state::{RunWord, StateWord};
use crate::render::{indent, prose, Glyphs, Look, INDENT};
use crate::run_page::{caution_row, RunPage};

impl RunPage<'_> {
    /// Every node the frozen workflow declares, in declaration order, a
    /// group's children one step under it.
    pub(crate) fn status_nodes(&self, look: &Look) -> Vec<Line> {
        let rows = self
            .frame
            .nodes
            .iter()
            .map(|node| {
                let display = crate::render::standing(&node.state);
                let said = display
                    .modifier
                    .map(|said| prose::first_sentence(&said, look.width.cells(), look.glyphs));
                let note = [said, handed(self.state, &node.id)]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(&format!(" {} ", look.glyphs.sep()));
                let id = format!("{}{}", indent(usize::from(node.group.is_some())), node.id);
                // The node a live engine is asking at its terminal is
                // waiting on a person, whatever its log says.
                let asking = self
                    .engine
                    .prompt
                    .as_ref()
                    .and_then(|prompt| prompt.node.as_ref());
                if asking == Some(&node.id) {
                    return NodeRow {
                        mark: StateWord::Waiting.mark(),
                        word: StateWord::Waiting.word(),
                        id,
                        note: "asking at its terminal".to_string(),
                    };
                }
                NodeRow {
                    mark: display.word.mark(),
                    word: display.word.word(),
                    id,
                    note,
                }
            })
            .collect();
        NodeTable { rows }.lines(look)
    }

    /// The last call of a run tool each node's attempt made that failed,
    /// one to a line under the table: what a node got wrong on its way
    /// is not why it stands where it does, so it is not its row's note.
    /// A node that finished is past it: how its documents were accepted
    /// says what the way there took.
    pub(crate) fn status_calls(&self) -> Vec<Line> {
        self.frame
            .nodes
            .iter()
            .filter(|node| {
                !matches!(
                    self.state.nodes.state(&node.id),
                    Some(NodeState::Finished { .. })
                )
            })
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
    pub(crate) fn status_evidence(&self, look: &Look) -> Vec<Line> {
        let mut lines = Vec::new();
        for node in &self.frame.nodes {
            let Some(NodeState::Failed { failure, .. }) = self.state.nodes.state(&node.id) else {
                continue;
            };
            let mut said = self.claim_cut_short(failure, look);
            said.extend(self.detail(failure, look));
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
        let claim = crate::render::failure::headline(failure);
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
                yunta_engine::ObjectStore::at(self.paths.run_dir).path_of(output),
                self.paths.cwd,
                self.paths.home,
            )
        });
        FailureDetail { failure, whole }.lines(look)
    }

    /// The facts a reader checks once they know where the run stands.
    pub(crate) fn status_fields(&self, glyphs: Glyphs) -> Fields {
        let mut tasks: Vec<_> = self.state.tasks.iter().collect();
        tasks.sort_by(|a, b| a.0.cmp(b.0));
        let sep = format!(" {} ", glyphs.sep());
        let tasks = tasks
            .into_iter()
            .map(|(id, record)| format!("{id} {}", record.status))
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
            .push_if("progress", crate::render::counter::line(self.frame, glyphs))
            .push_if("tokens", self.tokens().unwrap_or_default())
            .push_if("caution", caution_row(self.frame))
            .push_if("tasks", tasks)
            .push_if(
                "environment",
                self.state
                    .run
                    .environment_drift()
                    .map(|drift| yunta_render::outcome::environment_drift(&drift))
                    .unwrap_or_default(),
            )
            .push_if("host", suspended.unwrap_or_default())
            .push_if(
                "unread",
                crate::commands::unknown_kinds_note(&self.frame.unknown_kinds, glyphs)
                    .unwrap_or_default(),
            )
    }

    /// What a person types next, for where the run stands.
    pub(crate) fn status_next(&self) -> Next {
        // A run asking at its terminal is alive: what moves it is the
        // answer given there, and what stops it is what stops any run
        // that is moving.
        let word = match self.engine.prompt {
            Some(_) => RunWord::Running,
            None => crate::render::observed_word(self.frame, &self.engine),
        };
        Next {
            steps: advice::after(word, self.run_id.handle(), self.decided_escalation()),
        }
    }
}

/// How node `id` got its documents accepted once it finished, when that
/// took more than one handover: what it took is part of what it did.
pub(super) fn handed(state: &RunState, id: &yunta_core::NodeId) -> Option<String> {
    if !matches!(state.nodes.state(id), Some(NodeState::Finished { .. })) {
        return None;
    }
    let refused = state
        .artifacts
        .handed_over_by(id)
        .filter(|handed| handed.refusals > 0)
        .max_by_key(|handed| handed.submissions)?;
    Some(format!(
        "accepted on its {} handover ({} refused)",
        yunta_core::text::ordinal(refused.submissions),
        refused.refusals
    ))
}
