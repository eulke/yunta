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
//! than it can would be the silent degradation that the rule that no
//! degradation is silent forbids, so the reason travels with the change.

use std::io::Write;

use yunta_engine::Moment;

use crate::render::ink::Ink;
use crate::render::{duration, Glyphs, Look};

use super::{chronicle, write_line};

/// One line per event, written as the event arrives.
pub(super) struct Lines {
    out: Box<dyn Write + Send>,
    /// How the stream this writes to is painted.
    ink: Ink,
}

impl Lines {
    /// Opens the surface on `out`, announcing what the reader is getting
    /// instead of the live region and why.
    pub(super) fn open(mut out: Box<dyn Write + Send>, reason: &str) -> Self {
        write_line(
            &mut out,
            &format!("live view off ({reason}): one line per event"),
        );
        Self {
            out,
            ink: crate::render::stderr_ink(),
        }
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
    /// moment carries, and the words for it — and under a node that
    /// failed, what it printed, as a watched terminal keeps it. `run` is
    /// what the run is called by.
    ///
    /// Every moment, not only the ones a watched terminal keeps: a
    /// reader who arrives after the fact has no region to have watched,
    /// so the log is all there is and it is all written.
    pub(super) fn moment(&mut self, moment: &Moment, run: &str, glyphs: Glyphs) {
        let look = Look {
            glyphs,
            ink: self.ink,
            width: crate::render::stderr_width(),
        };
        let said = self.ink.paint(&chronicle::line(moment, &look));
        write_line(
            &mut self.out,
            &format!("[{}] {said}", duration(moment.elapsed)),
        );
        for line in chronicle::evidence(moment, run, &look) {
            write_line(&mut self.out, &self.ink.paint(&line));
        }
    }
}

/// The one line a parked run shows, produced once and read by every
/// surface that has to show it.
///
/// This checks the three places a reader meets it — the chronicle's own
/// line, the closing verdict, and the page a `status` prints — say the
/// same thing, for every reason a run can park on.
#[cfg(test)]
mod pause_reason_tests {
    use yunta_core::events::{Escalation, Fact, Failure, PauseReason, RunEvent, RunPausedPayload};
    use yunta_core::{NodeId, QuestionId, RunId};
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
            PauseReason::NoRoom {
                available: 8_200_000_000,
                need: 15_400_000_000,
                keep_free: 10_000_000_000,
            },
            PauseReason::LoopOverrun {
                node: NodeId::from("build"),
                cap: 8,
            },
            PauseReason::ExternalGate {
                url: "https://example.invalid/pull/1".to_string(),
            },
            PauseReason::UncertainOrphans {
                nodes: vec![NodeId::from("review")],
            },
            PauseReason::NodeFailed {
                node: NodeId::from("lint"),
                failure: Failure::Message {
                    outcome: "exit 1".into(),
                },
            },
            PauseReason::AwaitingReset {
                node: NodeId::from("build"),
                at: chrono::DateTime::UNIX_EPOCH,
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
                run: RunId::from("child"),
                child: Box::new(PauseReason::Cancelled),
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
            PauseReason::Recorded {
                sentence: "a sentence an older log stored".to_string(),
            },
        ];
        for reason in &all {
            match reason {
                PauseReason::Escalation(_)
                | PauseReason::Cancelled
                | PauseReason::CancelledAfterCrash
                | PauseReason::BudgetExhausted { .. }
                | PauseReason::NoRoom { .. }
                | PauseReason::LoopOverrun { .. }
                | PauseReason::ExternalGate { .. }
                | PauseReason::UncertainOrphans { .. }
                | PauseReason::NodeFailed { .. }
                | PauseReason::AwaitingReset { .. }
                | PauseReason::Blocked { .. }
                | PauseReason::GateAborted { .. }
                | PauseReason::ChildPaused { .. }
                | PauseReason::Questions { .. }
                | PauseReason::AnswersRefused { .. }
                | PauseReason::Recorded { .. } => {}
            }
        }
        all
    }

    #[test]
    fn a_pause_reason_renders_once_at_the_border() {
        for reason in every_reason() {
            let sentence = yunta_render::pause::sentence(&reason);
            assert!(
                !sentence.is_empty(),
                "a reason a reader is shown says something"
            );
            let payload = RunPausedPayload::new(&reason);
            assert_eq!(payload.pause, reason);

            let said = chronicle::say(
                &yunta_testkit::moment(
                    None,
                    yunta_engine::Happening::Run((&RunEvent::Paused(payload.clone())).into()),
                ),
                crate::render::Glyphs::Unicode,
            );
            // A limit's reset is nobody's to answer, so its line reads waiting.
            let word = if matches!(reason, PauseReason::AwaitingReset { .. }) {
                "waiting"
            } else {
                "needs you"
            };
            assert_eq!(
                said.text,
                yunta_core::text::aside(
                    "run",
                    &yunta_core::text::one_line(&format!("{word} — {sentence}"))
                )
            );

            let waiting = WaitingOn::Run {
                reason: payload.pause.clone(),
            };
            let one_line = yunta_core::text::one_line(&sentence);
            assert_eq!(advice::parked_on(&waiting), one_line);
            assert_eq!(advice::parked_in_full(&waiting), one_line);
        }
    }
}

/// The line a promotion says when its log names neither a cause nor a
/// sentence: the mode alone, on the chronicle line every surface writes.
#[cfg(test)]
mod promotion_tests {
    use yunta_core::events::{run, Evidence, Failure, PromotionCause};

    use super::super::chronicle;

    #[test]
    fn a_promotion_with_no_cause_recorded_reads_as_the_mode_alone() {
        let promoted = yunta_engine::Happening::Run(run::happening::Happening::PromotionSignaled {
            to: "ship".into(),
            cause: None,
            legacy_reason: None,
            evidence: Evidence::none(),
        });
        let said = chronicle::say(
            &yunta_testkit::moment(None, promoted),
            crate::render::Glyphs::Unicode,
        );
        assert_eq!(said.text, "run — promotion to `ship`");
    }

    /// The failure a promotion carries is read beside the node whose
    /// re-routes ran out, after the node and the mode it moves to.
    #[test]
    fn a_promotion_says_the_failure_that_exhausted_its_re_routes() {
        let cause = PromotionCause {
            node: "lint".into(),
            goto: "fix-lint".into(),
            failure: Some(Failure::Message {
                outcome: "exit 1".into(),
            }),
        };
        let promoted = yunta_engine::Happening::Run(run::happening::Happening::PromotionSignaled {
            to: "ship".into(),
            cause: Some(cause),
            legacy_reason: None,
            evidence: Evidence::none(),
        });
        let said = chronicle::say(
            &yunta_testkit::moment(None, promoted),
            crate::render::Glyphs::Unicode,
        );
        assert_eq!(
            said.text,
            "run — promotion to `ship`: node `lint` exhausted its re-routes to `fix-lint` — exit 1"
        );
    }
}
