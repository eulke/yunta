//! How far a run has come, in the one line every surface prints it in:
//! `status`, the run listing, the live region and the block that closes
//! a run.
//!
//! One function, because the same frame counted two ways is two answers
//! to one question — a page that said `1/2 nodes` while the block above
//! it said `nodes 0/2 · 1 fail` left a reader to work out which was
//! right.

use yunta_engine::{Counter, RunFrame};

use super::Glyphs;

/// The counters, at both levels the run has: the DAG's nodes and the
/// ledger's tasks, each as finished over what this run will do, with
/// every other bucket that holds anything beside it, and the re-routes
/// when there were any.
///
/// The buckets are not decoration. A task that fails at integration
/// goes back to ready and a re-routed node runs again, so `finished`
/// alone walks backwards; printed beside `failed`, `correcting`,
/// `running` and `waiting`, that same event reads as a move between
/// buckets, which is what makes a denominator that grew attributable to
/// the event that grew it.
pub(crate) fn line(frame: &RunFrame, glyphs: Glyphs) -> String {
    let sep = glyphs.sep();
    let mut line = format!("nodes {}", level(&frame.flow, sep));
    if let Some(tasks) = &frame.tasks {
        line.push_str(&format!(" {sep} tasks {}", level(tasks, sep)));
    }
    if frame.reroutes > 0 {
        line.push_str(&format!(
            " {sep} {}",
            yunta_core::text::counted(frame.reroutes, "reroute")
        ));
    }
    line
}

/// One level: finished over the total this run will do, then each
/// bucket that holds anything, then what the run's mode or the project
/// left out.
fn level(counter: &Counter, sep: char) -> String {
    let mut text = format!("{}/{}", counter.done, counter.total);
    for (count, word) in [
        (counter.failed, "failed"),
        (counter.correcting, "correcting"),
        (counter.running, "running"),
        (counter.waiting, "waiting"),
    ] {
        if count > 0 {
            text.push_str(&format!(" {sep} {count} {word}"));
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
                    failed: 1,
                    correcting: 1,
                    running: 1,
                    to_go: 2,
                    total: 5,
                    ..Counter::default()
                },
                1,
            ),
            Glyphs::Unicode,
        );
        assert_eq!(
            counted,
            "nodes 0/5 · 1 failed · 1 correcting · 1 running · 1 reroute"
        );
    }

    #[test]
    fn a_run_with_no_reroute_says_nothing_about_reroutes() {
        let counted = line(
            &frame(
                Counter {
                    done: 2,
                    total: 2,
                    ..Counter::default()
                },
                0,
            ),
            Glyphs::Unicode,
        );
        assert_eq!(counted, "nodes 2/2");
    }
}
