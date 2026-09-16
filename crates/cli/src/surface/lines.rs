//! The append-only surface: one line per moment of the run, nothing
//! redrawn.
//!
//! This is what a pipe, a CI log, `TERM=dumb` and a screen reader get.
//! It writes the same chronicle a watched terminal keeps above its
//! region, in the same words — it keeps every moment where a terminal
//! keeps what closed something, and lays each out for a reader who
//! arrives after the fact instead of one watching. Every line opens
//! with the run's elapsed time at that moment, so a log read back hours
//! later still says where the time went.
//!
//! Its first line names the downgrade. A surface that quietly did less
//! than it can would be the silent degradation the contract's invariant
//! I11 forbids, so the reason travels with the change.

use std::io::Write;

use yunta_engine::Moment;

use crate::render::{format_duration, Glyphs};

use super::{chronicle, write_line};

/// One line per event, written as the event arrives.
pub(super) struct Lines {
    out: Box<dyn Write + Send>,
}

impl Lines {
    /// Opens the surface on `out`, announcing what the reader is getting
    /// instead of the live region and why.
    pub(super) fn open(mut out: Box<dyn Write + Send>, reason: &str) -> Self {
        write_line(
            &mut out,
            &format!("live view off ({reason}): one line per event"),
        );
        Self { out }
    }

    /// Writes one diagnostic the run raised, as its own line.
    ///
    /// No elapsed time in front of it: the column an event's line opens
    /// with is that event's position in the run, and a diagnostic is
    /// not an event on the log.
    pub(super) fn note(&mut self, line: &str) {
        write_line(&mut self.out, line);
    }

    /// Writes `moment`'s line: the run's own clock, the mark the
    /// moment carries, and the words for it.
    ///
    /// Every moment, not only the ones a watched terminal keeps: a
    /// reader who arrives after the fact has no region to have watched,
    /// so the log is all there is and it is all written.
    pub(super) fn moment(&mut self, moment: &Moment, glyphs: Glyphs) {
        let said = chronicle::say(moment);
        let mark = said
            .word
            .map(|word| format!("{} ", glyphs.state(word)))
            .unwrap_or_default();
        write_line(
            &mut self.out,
            &format!("[{}] {mark}{}", format_duration(moment.elapsed), said.text),
        );
    }
}

/// The one line a parked run shows, produced once and read by every
/// surface that has to show it.
///
/// `PauseReason` is the fact; its `Display` is the only prose. This
/// checks the three places a reader meets it — the chronicle's own line,
/// the closing verdict, and the page a `status` prints — say the same
/// thing, for every reason a run can park on.
#[cfg(test)]
mod pause_reason_tests {
    use yunta_core::events::{Escalation, Fact, Failure, PauseReason, RunEvent, RunPausedPayload};
    use yunta_core::{NodeId, QuestionId};
    use yunta_engine::WaitingOn;

    use crate::commands::advice;

    use super::super::chronicle;

    /// One of every reason a run parks on. The `match` is what keeps it
    /// complete: a new variant stops compiling here until it is listed.
    fn every_reason() -> Vec<PauseReason> {
        let all = vec![
            PauseReason::Escalation(Box::new(
                Escalation::published_to(
                    "Ready to open the PR?",
                    vec![Fact::labelled("published at", "example/repo#1")].into(),
                    "https://example.invalid/pull/1",
                )
                .expect("the summary states no fact the evidence holds"),
            )),
            PauseReason::Cancelled,
            PauseReason::CancelledAfterCrash,
            PauseReason::BudgetExhausted {
                spent: 120_000,
                cap: 100_000,
            },
            PauseReason::LoopOverrun {
                node: NodeId::from("build"),
                cap: 8,
            },
            PauseReason::ExternalGate {
                url: "https://example.invalid/pull/1".to_string(),
            },
            PauseReason::UncertainOrphans(vec![NodeId::from("review")]),
            PauseReason::NodeFailed {
                node: NodeId::from("lint"),
                failure: Failure::message("exit 1"),
            },
            PauseReason::Blocked {
                node: NodeId::from("ship"),
                on: vec![NodeId::from("lint")],
            },
            PauseReason::GateAborted {
                node: NodeId::from("approve"),
                free_text: Some("not this quarter".to_string()),
            },
            PauseReason::ChildPaused {
                node: NodeId::from("fanout"),
                reason: "cancelled by user".to_string(),
            },
            PauseReason::Questions {
                node: NodeId::from("scope"),
                pending: (QuestionId::from("q1"), Vec::new()).into(),
            },
            PauseReason::AnswersRefused {
                node: NodeId::from("scope"),
                report: yunta_core::diagnostic::Report::new(
                    yunta_core::diagnostic::DocumentRef::new(
                        yunta_core::ArtifactKind::Answers,
                        "answers.yaml",
                    ),
                    vec![yunta_core::diagnostic::Diagnostic::new(
                        yunta_core::diagnostic::Subject::Question(
                            yunta_core::diagnostic::Named::new(
                                yunta_core::QuestionId::from("q2"),
                                0,
                            ),
                        ),
                        yunta_core::diagnostic::Problem::rule(
                            yunta_core::diagnostic::RuleCode::MissingAnswer,
                            "this question is `required` and nothing answers it",
                        ),
                    )],
                ),
            },
        ];
        for reason in &all {
            match reason {
                PauseReason::Escalation(_)
                | PauseReason::Cancelled
                | PauseReason::CancelledAfterCrash
                | PauseReason::BudgetExhausted { .. }
                | PauseReason::LoopOverrun { .. }
                | PauseReason::ExternalGate { .. }
                | PauseReason::UncertainOrphans(_)
                | PauseReason::NodeFailed { .. }
                | PauseReason::Blocked { .. }
                | PauseReason::GateAborted { .. }
                | PauseReason::ChildPaused { .. }
                | PauseReason::Questions { .. }
                | PauseReason::AnswersRefused { .. } => {}
            }
        }
        all
    }

    #[test]
    fn a_pause_reason_renders_once_at_the_border() {
        for reason in every_reason() {
            let sentence = reason.to_string();
            assert!(
                !sentence.is_empty(),
                "a reason a reader is shown says something"
            );
            let payload = RunPausedPayload::new(&reason);
            assert_eq!(payload.reason(), sentence);

            let said = chronicle::say(&yunta_testkit::moment(
                None,
                yunta_engine::Happening::Run((&RunEvent::Paused(payload.clone())).into()),
            ));
            assert_eq!(
                said.text,
                yunta_core::text::aside(
                    "run",
                    &yunta_core::text::one_line(&format!("paused — {sentence}"))
                )
            );

            let waiting = WaitingOn::Run {
                reason: payload.reason().to_string(),
            };
            let one_line = yunta_core::text::one_line(&sentence);
            assert_eq!(advice::parked_on(&waiting), one_line);
            assert_eq!(advice::parked_in_full(&waiting), one_line);
        }
    }
}
