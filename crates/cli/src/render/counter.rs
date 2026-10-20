//! How far a run has come, in the one line every surface prints it in:
//! `status`, the run listing, the live region and the block that closes
//! a run.
//!
//! One function, because the same frame counted two ways is two answers
//! to one question — a page that said `1/2 nodes` while the block above
//! it said `nodes 0/2 · 1 fail` left a reader to work out which was
//! right.

use surco::glyphs::Glyphs;
use surco::text::counted;
use surco::words::Standing;
use yunta_engine::{Counter, RunFrame, TaskCounter};

/// The counters, at both levels the run has: the DAG's nodes and the
/// run's tasks, each as finished over what this run will do, with
/// every other place that holds anything beside it, and the re-routes
/// when there were any.
///
/// The places are not decoration. A re-routed node runs again, so
/// `finished` alone walks backwards; printed beside `failed`, `rerouted`
/// and `running`, that same event reads as a move between places, which
/// is what makes a denominator that grew attributable to the event that
/// grew it. Each node is counted under the word its row says.
pub(crate) fn line(frame: &RunFrame, glyphs: Glyphs) -> String {
    let sep = glyphs.sep();
    let mut line = format!("nodes {}", nodes(&frame.flow, sep));
    if let Some(tasks) = &frame.tasks {
        line.push_str(&format!(" {sep} tasks {}", self::tasks(tasks, sep)));
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

/// The nodes: finished over the total this run will do, then each other
/// place a node stands in, then what the run's mode or the project left
/// out.
fn nodes(counter: &Counter, sep: char) -> String {
    let mut text = format!("{}/{}", counter.of(Standing::Finished), counter.total);
    for standing in NAMED {
        let count = counter.of(standing);
        if count > 0 {
            text.push_str(&format!(" {sep} {count} {standing}"));
        }
    }
    if let Some(mode) = &counter.skipped_by {
        text.push_str(&format!(" {sep} {} skipped by `{mode}`", counter.skipped));
    }
    if counter.left_out > 0 {
        text.push_str(&format!(" {sep} {} left out", counter.left_out));
    }
    text
}

/// The tasks: done over the total, then each status that holds anything,
/// in the words a task's ledger calls it by.
fn tasks(counter: &TaskCounter, sep: char) -> String {
    let mut text = format!("{}/{}", counter.done, counter.total);
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
            "nodes 0/5 · 1 failed · 1 rerouted · 1 running · 1 reroute"
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
        assert_eq!(counted, "nodes 2/2");
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
            "nodes 0/0 · tasks 1/3 · 1 blocked"
        );
    }
}
