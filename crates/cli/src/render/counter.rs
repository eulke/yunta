//! How far a run has come, in the one line every surface prints it in:
//! `status`, the run listing, the live region and the block that closes
//! a run.
//!
//! One owner, because the same frame counted two ways is two answers to
//! one question: a page that said one count while the block above it said
//! another left a reader to work out which was right. Every count reads
//! `N of M`.

use std::collections::BTreeMap;

use surco::glyphs::{Glyphs, Mark};
use surco::text::counted;
use surco::words::Standing;
use yunta_core::events::{TaskLedger, TaskStatus};
use yunta_engine::{Counter, RunFrame, TaskCounter};

/// How many names a row of a page says before it counts the rest: enough
/// to recognise a place by, few enough that the row stays one row. Every
/// name is in the table `--full` draws.
const NAMES: usize = 3;

/// How far a run has come, as its page's `progress` row says it: the
/// nodes done of the ones this run will do, what narrowed that total, and
/// the re-routes that grew it. Where each other node stands is said by
/// [`places`], with its name.
pub(crate) fn progress(frame: &RunFrame, glyphs: Glyphs) -> String {
    let sep = glyphs.sep();
    let flow = &frame.flow;
    let mut text = format!("{}{}", done(flow), narrowed(flow, sep));
    if frame.reroutes > 0 {
        text.push_str(&format!(" {sep} {}", counted(frame.reroutes, "reroute")));
    }
    text
}

/// Every place the run's nodes stand in, as a page's `nodes` rows say
/// them: the mark, how many stand there, the word, and the first of their
/// names. In the furrow's order: what the run left behind, where it is
/// now, what is ahead of it. Nothing when every node finished, which the
/// progress row already says.
pub(crate) fn places(frame: &RunFrame) -> Vec<(Mark, String)> {
    let mut places: BTreeMap<(u8, Standing), Vec<&str>> = BTreeMap::new();
    // What the run's mode or project leaves out is not work this run will
    // do: the progress row counts it apart.
    for node in frame
        .nodes
        .iter()
        .filter(|node| node.standing != Standing::Skipped)
    {
        let ahead = match node.standing {
            Standing::Finished => 0,
            Standing::NeverRan => 2,
            _ => 1,
        };
        places
            .entry((ahead, node.standing))
            .or_default()
            .push(node.id.as_str());
    }
    if places
        .keys()
        .all(|(_, standing)| *standing == Standing::Finished)
    {
        return Vec::new();
    }
    places
        .into_iter()
        .map(|((_, standing), names)| {
            (
                standing.mark(),
                format!("{} {standing}: {}", names.len(), named(&names)),
            )
        })
        .collect()
}

/// The run's tasks, as a page's `tasks` row says them: done of the total,
/// then each status that stops or holds a task, with the tasks it holds
/// named. `None` for a run with no tasks.
pub(crate) fn tasks_row(frame: &RunFrame, ledger: &TaskLedger, glyphs: Glyphs) -> Option<String> {
    let counter = frame.tasks.as_ref()?;
    let sep = glyphs.sep();
    let mut text = format!("{} of {} done", counter.done, counter.total);
    for status in [TaskStatus::Failed, TaskStatus::Blocked, TaskStatus::Running] {
        let mut held: Vec<&str> = ledger
            .iter()
            .filter(|(_, record)| record.status == status)
            .map(|(id, _)| id.as_str())
            .collect();
        held.sort_unstable();
        if !held.is_empty() {
            text.push_str(&format!(
                " {sep} {} {}: {}",
                held.len(),
                status.as_str(),
                named(&held)
            ));
        }
    }
    Some(text)
}

/// The first [`NAMES`] of `names`, and how many more there are.
fn named(names: &[&str]) -> String {
    let mut said: Vec<String> = names
        .iter()
        .take(NAMES)
        .map(|name| name.to_string())
        .collect();
    if names.len() > NAMES {
        said.push(format!("+{}", names.len() - NAMES));
    }
    said.join(", ")
}

/// How many nodes are done of the ones this run will do.
fn done(counter: &Counter) -> String {
    format!(
        "{} of {} done",
        counter.of(Standing::Finished),
        counted(counter.total, "node")
    )
}

/// What narrowed the nodes this run will do: the ones its mode skips and
/// the ones its project leaves out, each after `sep`.
fn narrowed(counter: &Counter, sep: char) -> String {
    let mut text = String::new();
    if let Some(mode) = &counter.skipped_by {
        text.push_str(&format!(" {sep} {} skipped by `{mode}`", counter.skipped));
    }
    if counter.left_out > 0 {
        text.push_str(&format!(" {sep} {} left out", counter.left_out));
    }
    text
}

/// The counters, at both levels the run has: the DAG's nodes and the
/// run's tasks, each as `N of M done`, with every other place that holds
/// anything beside it, and the re-routes when there were any.
///
/// The places are not decoration. A re-routed node runs again, so
/// `finished` alone walks backwards; printed beside `failed`, `rerouted`
/// and `running`, that same event reads as a move between places, which
/// is what makes a denominator that grew attributable to the event that
/// grew it. Each node is counted under the word its row says.
pub(crate) fn line(frame: &RunFrame, glyphs: Glyphs) -> String {
    let sep = glyphs.sep();
    let mut line = nodes(&frame.flow, sep);
    if let Some(tasks) = &frame.tasks {
        line.push_str(&format!(" {sep} {}", self::tasks(tasks, sep)));
    }
    if frame.reroutes > 0 {
        line.push_str(&format!(" {sep} {}", counted(frame.reroutes, "reroute")));
    }
    line
}

/// The places a node stands in that the line names beside what finished,
/// in the order it names them: what went wrong, then what moves, then
/// what is held. What never ran is what the total already says is left.
const NAMED: [Standing; 6] = [
    Standing::Failed,
    Standing::Rerouted,
    Standing::Running,
    Standing::Stopped,
    Standing::Waiting,
    Standing::NeedsYou,
];

/// The nodes: how many are done of the ones this run will do, then each
/// other place a node stands in, then what the run's mode or the project
/// left out.
fn nodes(counter: &Counter, sep: char) -> String {
    let mut text = done(counter);
    for standing in NAMED {
        let count = counter.of(standing);
        if count > 0 {
            text.push_str(&format!(" {sep} {count} {standing}"));
        }
    }
    text.push_str(&narrowed(counter, sep));
    text
}

/// The tasks: how many are done of the total, then each status that holds
/// anything, in the words a task's ledger calls it by.
fn tasks(counter: &TaskCounter, sep: char) -> String {
    let mut text = format!(
        "{} of {} done",
        counter.done,
        counted(counter.total, "task")
    );
    for (count, word) in [
        (counter.failed, "failed"),
        (counter.running, "running"),
        (counter.blocked, "blocked"),
    ] {
        if count > 0 {
            text.push_str(&format!(" {sep} {count} {word}"));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::RunId;
    use yunta_testkit::run_frame;

    fn frame(flow: Counter, reroutes: usize) -> RunFrame {
        let mut frame = run_frame(&RunId::from("01J"));
        frame.flow = flow;
        frame.reroutes = reroutes;
        frame
    }

    #[test]
    fn a_node_sent_to_be_corrected_is_counted_apart_from_one_that_failed() {
        let counted = line(
            &frame(
                Counter {
                    by: vec![
                        (Standing::Running, 1),
                        (Standing::Failed, 1),
                        (Standing::Rerouted, 1),
                        (Standing::NeverRan, 2),
                    ],
                    total: 5,
                    ..Counter::default()
                },
                1,
            ),
            Glyphs::Unicode,
        );
        assert_eq!(
            counted,
            "0 of 5 nodes done · 1 failed · 1 rerouted · 1 running · 1 reroute"
        );
    }

    #[test]
    fn a_run_with_no_reroute_says_nothing_about_reroutes() {
        let counted = line(
            &frame(
                Counter {
                    by: vec![(Standing::Finished, 2)],
                    total: 2,
                    ..Counter::default()
                },
                0,
            ),
            Glyphs::Unicode,
        );
        assert_eq!(counted, "2 of 2 nodes done");
    }

    #[test]
    fn a_blocked_task_is_counted_blocked_never_waiting() {
        let mut frame = frame(Counter::default(), 0);
        frame.tasks = Some(TaskCounter {
            done: 1,
            blocked: 1,
            to_go: 1,
            total: 3,
            ..TaskCounter::default()
        });
        assert_eq!(
            line(&frame, Glyphs::Unicode),
            "0 of 0 nodes done · 1 of 3 tasks done · 1 blocked"
        );
    }
}
