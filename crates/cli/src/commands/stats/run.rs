//! `yunta stats <run>`: where one run's time went, by node and by runner,
//! what it spent, and what it handed over and found.

use std::collections::BTreeMap;
use std::time::Duration;

use surco::bars::{bar_cells, fine_bar, FineBar, BAR_LEAST};
use surco::components::{Fields, Furrow, Gist, Headline};
use surco::doc::{Block, Depth, Doc};
use surco::figures::{duration, Ratio, Tokens, DURATION_WIDEST};
use surco::glyphs::Glyphs;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use surco::width::{cell_width, id_column, middle_cut, truncate, INDENT, INDENT_WIDTH};
use surco::words::{Standing, WAITING_ON_YOU};
use yunta_core::NodeId;
use yunta_engine::{NodeFrame, NodeStat, RunFrame, RunPhase, RunStats};
use yunta_render::state::NodeDisplay;

use crate::render::node_display;

use super::facts::{answers, cache, cptv, findings, rework, spent, submissions};

/// What the `yunta stats <run_id>` block is drawn from: one run's stats,
/// the run as its frame stands it, what its model is priced at, and how
/// much of it the reader asked for.
pub(super) struct RunStatsPage<'a> {
    /// The run's word, workflow, mode and nodes: the word a node's row
    /// says here is the one `status` says.
    pub(super) frame: &'a RunFrame,
    pub(super) stats: &'a RunStats,
    pub(super) pricing: Option<&'a BTreeMap<String, yunta_core::PricingEntry>>,
    /// `--full` adds what is the run's own business: its rails, and each
    /// node's blocked share and models.
    pub(super) depth: Depth,
}

impl RunStatsPage<'_> {
    /// The whole `yunta stats <run_id>` block: the run's headline and
    /// furrow, where most of its time went, its facts — a row each that
    /// has something to say — then its time, at `--full` its rails, its
    /// nodes and its runners.
    ///
    /// A block rather than a run of `println!`s: what this command says
    /// about a run is one thing a test reads whole, and a number nobody can
    /// assert is a number nobody is holding to anything.
    pub(super) fn document(&self, look: &Look) -> Doc {
        let stats = self.stats;
        let asleep = match stats.asleep.is_zero() {
            true => String::new(),
            false => format!("{}, left out of every duration", duration(stats.asleep)),
        };
        let fields = Fields::new()
            .push_if("task cost", cptv(stats, look.glyphs))
            .push_if("rework", rework(stats, look.glyphs))
            .push_if("cache", cache(stats, look.glyphs))
            .push_if("tokens", spent(stats))
            .push_if(
                "cost",
                super::currency_line(stats.total_tokens.total(), self.pricing).unwrap_or_default(),
            )
            .push_if("host asleep", asleep)
            .push_if("documents", submissions(&stats.artifact_submissions))
            .push_if(
                "findings",
                findings(&stats.findings, stats.findings_effective),
            )
            .push_if("answers", answers(&stats.findings, stats.findings_settled))
            .push_if(
                "unread",
                yunta_render::receipt::unknown_kinds(&stats.unknown_kinds).unwrap_or_default(),
            );
        let furrow = self.frame.nodes.iter().map(|node| node.standing).collect();
        let gist = super::time::gist(&stats.time, self.person());
        let mut doc = Doc::new()
            .with(self.headline())
            .with(Furrow(furrow))
            .with_all(gist.map(|gist| Gist::new(gist).into()))
            .with(fields)
            .with(Block::Lines(super::time::render(&stats.time, look)));
        if self.depth.reaches(Depth::Full) {
            doc = doc.with(Block::Lines(super::time::rail_latency(&stats.rails, look)));
        }
        doc.with(Block::Lines(self.nodes_lines(look)))
            .with(Block::Lines(runner_lines(stats, look)))
    }

    /// The node a person was asked about longest, when it holds most of a
    /// person's part of the run: where the gist says that part went. A part
    /// spread over nodes, or held by asks of the run itself, names none.
    fn person(&self) -> Option<&NodeId> {
        let (node, waited) = self
            .stats
            .people_by_node
            .iter()
            .max_by_key(|(_, waited)| **waited)?;
        (waited.as_secs_f64() * 2.0 > self.stats.time.people.as_secs_f64()).then_some(node)
    }
}

impl RunStatsPage<'_> {
    /// The run's headline: its mark, workflow and mode, its word, how long
    /// it has run — so far, or in all once it closed — and its handle.
    fn headline(&self) -> Headline {
        let frame = self.frame;
        let word = frame.standing;
        let mut headline = Headline::new(
            word.mark(),
            format!("{} ({})", frame.workflow, frame.mode),
            word.word(),
        );
        let open = matches!(
            frame.phase,
            RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
        );
        if let Some(took) = frame.elapsed {
            headline = headline.fact(match open {
                true => format!("{} so far", duration(took)),
                false => format!("in {}", duration(took)),
            });
        }
        headline.edge(frame.run_id.handle())
    }

    /// One row per node that started, opening with where it stands: its id,
    /// how long it worked or a person took on it, a bar of that time, and
    /// what it took past its first attempt and spent. The bars share one
    /// scale, the longest work of a node; a person's wait reads in the
    /// person's tone, cut where it runs past the scale. `--full` adds, under
    /// each node that has them, its blocked share and its models. Nothing
    /// for a run whose nodes never started.
    fn nodes_lines(&self, look: &Look) -> Vec<Line> {
        if self.stats.nodes.is_empty() {
            return Vec::new();
        }
        let rows: Vec<NodeRow<'_>> = self.stats.nodes.iter().map(|stat| self.row(stat)).collect();
        let (rows, shape) = fitted(rows, look, self.depth.reaches(Depth::Dig));
        let full = self.depth.reaches(Depth::Full);
        let mut lines = vec![Line::new(), Line::new().plain("nodes:")];
        for row in &rows {
            lines.push(node_line(row, &shape, look.glyphs));
            if full {
                lines.extend(private(row.stat, look));
            }
        }
        lines
    }

    /// One node's row, read before it is drawn.
    fn row<'a>(&self, stat: &'a NodeStat) -> NodeRow<'a> {
        let frame = self
            .frame
            .nodes
            .iter()
            .find(|frame| frame.id == stat.node_id);
        let display = frame.map_or_else(|| NodeDisplay::of(Standing::NeverRan, None), node_display);
        let person = frame.is_some_and(NodeFrame::person_took);
        NodeRow {
            stat,
            display,
            took: match (person, frame) {
                (true, Some(frame)) => frame.people,
                _ => stat.active_so_far(),
            },
            person,
            rounds: frame.map_or(0, NodeFrame::person_rounds),
            attempts: match stat.attempts {
                0 | 1 => String::new(),
                n => counted(n as usize, "attempt"),
            },
        }
    }
}

/// One node's row, read before it is drawn: its stat, how its word reads,
/// the time the row shows and whether a person took it, the rounds a person
/// decided it in, and its attempts past the first.
struct NodeRow<'a> {
    stat: &'a NodeStat,
    display: NodeDisplay,
    took: Duration,
    person: bool,
    rounds: usize,
    attempts: String,
}

impl NodeRow<'_> {
    /// This row without its attempts and its rounds: what a line too narrow
    /// for a bar beside them says of it.
    fn lean(self) -> Self {
        Self {
            attempts: String::new(),
            rounds: 0,
            ..self
        }
    }
}

/// What every node row shares: the bars' scale, the id column's width, the
/// cells a bar takes, the attempts column's width, and whether the widest
/// row fits the line without a bar.
struct Shape {
    scale: Duration,
    column: usize,
    cells: usize,
    attempts: usize,
    fits: bool,
}

/// The layout every node row shares on the line `look` draws. The id is
/// whole where the row has room for it beside a bar; on a narrower line it
/// gives way, never below a third of the line, and the bar goes before the
/// id does. At `dig` the id is always whole. The bars' scale is the longest
/// work of a node: a person's wait is no work.
fn shape_of(rows: &[NodeRow<'_>], look: &Look, dig: bool) -> Shape {
    let width = look.width.cells();
    let scale = rows
        .iter()
        .filter(|row| !row.person)
        .map(|row| row.took)
        .max()
        .unwrap_or_default();
    let attempts = rows
        .iter()
        .map(|row| cell_width(&row.attempts))
        .max()
        .unwrap_or(0);
    let tail = rows
        .iter()
        .map(|row| {
            let (before, last) = tail_of(row, attempts);
            cell_width(&before) + cell_width(&last)
        })
        .max()
        .unwrap_or(0);
    // The margin, the mark and the word with their gaps before the id, the
    // gap and the time after it, and what the widest row says after its bar.
    let fixed = UNDER_ID + 2 + DURATION_WIDEST + tail;
    let longest = id_column(rows.iter().map(|row| row.stat.node_id.as_str()));
    // The id is whole where the widest row leaves it room beside a bar of
    // `BAR_LEAST` cells, and gives way to that room, never below a third of
    // the line.
    let column = match dig {
        true => longest,
        false => width
            .saturating_sub(fixed + 1 + BAR_LEAST)
            .clamp(longest.min(width / 3), longest),
    };
    // The bar's own space comes before its cells.
    let cells = bar_cells(width, fixed + column + 1);
    Shape {
        scale,
        column,
        cells,
        attempts,
        fits: fixed + column <= width,
    }
}

/// The rows and the shape they are drawn with. Rows that fit the line keep
/// their attempts and their rounds, with or without a bar: the bar repeats
/// the time a row says in words, and goes first. Only where the widest row
/// runs past the line without a bar, its id already at its least, does every
/// row leave its attempts and its rounds out; the trace still says them.
fn fitted<'a>(rows: Vec<NodeRow<'a>>, look: &Look, dig: bool) -> (Vec<NodeRow<'a>>, Shape) {
    let shape = shape_of(&rows, look, dig);
    if dig || shape.fits {
        return (rows, shape);
    }
    let lean: Vec<NodeRow<'a>> = rows.into_iter().map(NodeRow::lean).collect();
    let shape = shape_of(&lean, look, dig);
    (lean, shape)
}

/// Where a node row's id starts: the margin, the mark, and the word with
/// its gaps.
const UNDER_ID: usize = INDENT_WIDTH + 2 + Standing::WIDEST + 1;

/// What a row says after its bar, in two parts: its attempts past the first,
/// padded to `attempts` cells between two gaps, and then a person's wait, or
/// the tokens the node spent, nothing for a node that spent none.
fn tail_of(row: &NodeRow<'_>, attempts: usize) -> (String, String) {
    let last = match (row.person, row.stat.tokens.total()) {
        (true, _) => waiting_on_you(row.rounds),
        (false, 0) => String::new(),
        (false, spent) => Tokens(spent).column(),
    };
    (format!("  {:<attempts$}  ", row.attempts), last)
}

/// The same tokens grouped by the runner that spent them: where the
/// run's cost went, across however many nodes each runner was given, each
/// with a fine bar against the runner that spent the most.
fn runner_lines(stats: &RunStats, look: &Look) -> Vec<Line> {
    // A runner that spent nothing says nothing.
    let by_runner: Vec<_> = stats
        .tokens_by_runner()
        .into_iter()
        .filter(|(_, tokens)| tokens.total() > 0)
        .collect();
    if by_runner.is_empty() {
        return Vec::new();
    }
    let most = by_runner.iter().map(|(_, t)| t.total()).max().unwrap_or(0);
    let width = look.width.cells();
    let column = id_column(by_runner.iter().map(|(runner, _)| runner.as_str())).min(width / 3);
    let cells = bar_cells(width, cell_width(INDENT) + column + Tokens::WIDEST + 10);
    let mut lines = vec![Line::new(), Line::new().plain("runners:")];
    for (runner, tokens) in &by_runner {
        let total = tokens.total();
        let line = Line::new().plain(format!(
            "{INDENT}{}",
            middle_cut(runner.as_str(), column, look.glyphs)
        ));
        let line = measured(
            line,
            total as f64,
            most as f64,
            cells,
            (Tone::Running, look.glyphs),
        );
        lines.push(line.plain(format!("  {}", Tokens(total).column().trim_end())));
    }
    lines
}

/// `line` with a fine bar of `value` against `scale` after a space, `cells`
/// wide and padded to them, in `tone`; nothing where the line has no room
/// for a bar. A value past its scale is the whole track, faint, with the cut
/// over its middle in `tone`.
fn measured(
    line: Line,
    value: f64,
    scale: f64,
    cells: usize,
    (tone, glyphs): (Tone, Glyphs),
) -> Line {
    if cells == 0 {
        return line;
    }
    match fine_bar(value, scale, cells, glyphs) {
        FineBar::Measured(drawn) => {
            let pad = cells.saturating_sub(cell_width(&drawn));
            line.plain(" ").push(tone, drawn).plain(" ".repeat(pad))
        }
        FineBar::Cut { before, cut, after } => line
            .plain(" ")
            .push(Tone::Faint, before)
            .push(tone, cut)
            .push(Tone::Faint, after),
    }
}

/// One node's row: its mark in its tone, its word, its id cut to the
/// shape's column, its time, a bar of that time, and its attempts and its
/// tokens; a person's wait in the person's tone, after its attempts.
fn node_line(row: &NodeRow<'_>, shape: &Shape, glyphs: Glyphs) -> Line {
    let mark = row.display.standing.mark();
    let (time_tone, bar_tone, last_tone) = match row.person {
        true => (Tone::NeedsYou, Tone::NeedsYou, Tone::NeedsYou),
        false => (Tone::Plain, Tone::of(mark), Tone::Plain),
    };
    let line = Line::new()
        .plain(INDENT)
        .push(Tone::of(mark), glyphs.mark(mark).to_string())
        .plain(format!(
            " {} {}  ",
            truncate(row.display.standing.word(), Standing::WIDEST, glyphs),
            middle_cut(row.stat.node_id.as_str(), shape.column, glyphs),
        ))
        .push(
            time_tone,
            format!("{:>DURATION_WIDEST$}", duration(row.took)),
        );
    let line = measured(
        line,
        row.took.as_secs_f64(),
        shape.scale.as_secs_f64(),
        shape.cells,
        (bar_tone, glyphs),
    );
    let (before, last) = tail_of(row, shape.attempts);
    match last.is_empty() {
        true => line.plain(before.trim_end()),
        false => line.plain(before).push(last_tone, last),
    }
}

/// What a row of a person's wait says: whose time it was, and the rounds a
/// person had with the node when more than one.
fn waiting_on_you(rounds: usize) -> String {
    match rounds {
        0 | 1 => WAITING_ON_YOU.to_string(),
        n => format!("{WAITING_ON_YOU}, {}", counted(n, "round")),
    }
}

/// What the whole story adds of a node, on a muted line under its row from
/// where its id starts: its blocked share, when it was blocked at all, and
/// its models, cut to the line. Nothing for a node that has neither.
fn private(stat: &NodeStat, look: &Look) -> Option<Line> {
    let glyphs = look.glyphs;
    let blocked = stat
        .blocked_fraction()
        .filter(|fraction| *fraction > 0.0)
        .map(|fraction| format!("{} blocked", Ratio(fraction).reads(glyphs.times())));
    let models = (!stat.models.is_empty()).then(|| {
        stat.models
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    });
    let said: Vec<String> = blocked.into_iter().chain(models).collect();
    if said.is_empty() {
        return None;
    }
    let room = look.width.cells().saturating_sub(UNDER_ID);
    let separator = format!(" {} ", glyphs.sep());
    let said = said.join(separator.as_str());
    let cut = truncate(&said, room, glyphs);
    Some(
        Line::new()
            .plain(" ".repeat(UNDER_ID))
            .push(Tone::Muted, cut.trim_end()),
    )
}

#[cfg(test)]
mod tests;
