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

use surco::figures::duration;
use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::paint::Paint;
use yunta_replay::frame::Moment;

use super::{chronicle, write_line};

/// One line per event, written as the event arrives.
pub(super) struct Lines {
    out: Box<dyn Write + Send>,
    /// How the stream this writes to is painted.
    paint: Paint,
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
            paint: crate::render::stderr_paint(),
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
    /// Every moment that says anything, not only the ones a watched terminal keeps: a
    /// reader who arrives after the fact has no region to have watched,
    /// so the log is all there is and it is all written.
    pub(super) fn moment(&mut self, moment: &Moment, run: &str, glyphs: Glyphs) {
        if !chronicle::says_anything(&moment.happening) {
            return;
        }
        let look = Look {
            glyphs,
            paint: self.paint,
            width: crate::render::stderr_width(),
        };
        let said = self.paint.line(&chronicle::line(moment, &look));
        write_line(
            &mut self.out,
            &format!("[{}] {said}", duration(moment.elapsed)),
        );
        for line in chronicle::evidence(moment, run, &look) {
            write_line(&mut self.out, &self.paint.line(&line));
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
    use yunta_core::events::{
        Claim, Escalation, Failure, PauseReason, PublishedGate, RunEvent, RunPausedPayload,
    };
    use yunta_core::{NodeId, QuestionId, RunId};
    use yunta_replay::frame::WaitingOn;

    use crate::commands::advice;

    use super::super::chronicle;
    use super::Lines;

    /// One of every reason a run parks on. The `match` is what keeps it
    /// complete: a new variant stops compiling here until it is listed.
    fn every_reason() -> Vec<PauseReason> {
        let mut all = gates_and_budgets();
        all.extend(nodes_and_children());
        all.extend(answers_and_older_logs());
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
                | PauseReason::PullRequestClosed { .. }
                | PauseReason::ChildPaused { .. }
                | PauseReason::Questions { .. }
                | PauseReason::AnswersRefused { .. }
                | PauseReason::Recorded { .. } => {}
            }
        }
        all
    }

    /// The reasons a person decides on, and the caps a run can reach.
    fn gates_and_budgets() -> Vec<PauseReason> {
        vec![
            PauseReason::Escalation(Box::new(Escalation::published_to(
                Claim::AwaitsReview {
                    node: NodeId::from("pr"),
                    assignee: Some("lead".to_string()),
                },
                PublishedGate {
                    url: "https://example.invalid/pull/1".to_string(),
                    folder: None,
                },
                "https://example.invalid/pull/1",
            ))),
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
        ]
    }

    /// What the run waits on at its nodes and children.
    fn nodes_and_children() -> Vec<PauseReason> {
        vec![
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
            PauseReason::PullRequestClosed {
                node: NodeId::from("approve"),
                url: "https://mock.forge/pr/1".to_string(),
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
        ]
    }

    /// An answer the engine refused, and a sentence an older log stored.
    fn answers_and_older_logs() -> Vec<PauseReason> {
        vec![
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
                        yunta_core::diagnostic::Problem::broke(
                            yunta_core::QuestionFault::MissingAnswer,
                        ),
                    )],
                ),
            },
            PauseReason::Recorded {
                sentence: "a sentence an older log stored".into(),
            },
        ]
    }

    /// What `failure` writes on an append-only surface, and whether a watched
    /// terminal keeps it.
    fn said_of(failure: yunta_core::events::Failure) -> (String, bool) {
        use yunta_core::events::{
            EventBody, EventPayload, NodeEvent, NodeFailedPayload, StoredEvent, TokenUsage,
        };
        let event = StoredEvent {
            seq: 1.into(),
            run_id: "01JQ0000000000000000000000".into(),
            node_id: Some("approve".into()),
            timestamp: chrono::DateTime::UNIX_EPOCH,
            body: EventBody::Known(EventPayload::Node(NodeEvent::Failed(
                NodeFailedPayload::new(failure, TokenUsage::default()),
            ))),
        };
        let moments = yunta_replay::frame::chronicle(&[event]);
        let moment = moments.first().expect("one moment per event");
        let written = yunta_testkit_core::Captured::default();
        let mut lines = Lines {
            out: Box::new(written.clone()),
            paint: surco::paint::Paint::Plain,
        };
        lines.moment(moment, "Y0P5", surco::glyphs::Glyphs::Unicode);
        (written.text(), chronicle::kept(&moment.happening))
    }

    /// A person choosing an option that goes back is no failure on any surface: the node's
    /// reroute is the line that says it, with what the person wrote. A node that did fail
    /// still says so on both.
    #[test]
    fn a_choice_that_goes_back_is_said_by_its_reroute_and_not_as_a_failure() {
        let (written, kept) = said_of(yunta_core::events::Failure::GateChose {
            node: "approve".into(),
            option: "adjust".into(),
            target: "plan".into(),
            said: Some("keep the lock".to_string()),
        });
        assert_eq!(written, "");
        assert!(!kept);

        let (written, kept) = said_of(yunta_core::events::Failure::Message {
            outcome: "exit 1".into(),
        });
        assert!(written.contains("approve failed"), "{written}");
        assert!(kept);
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
                &yunta_replay::frame::Moment::sample(
                    None,
                    yunta_replay::frame::Happening::Run(
                        (&RunEvent::Paused(payload.clone())).into(),
                    ),
                ),
                surco::glyphs::Glyphs::Unicode,
            );
            // A limit's reset is nobody's to answer, so its line reads waiting;
            // a person's own stop reads stopped.
            let word = if matches!(reason, PauseReason::AwaitingReset { .. }) {
                "waiting"
            } else if reason.by_a_person() {
                "stopped"
            } else {
                "needs you"
            };
            assert_eq!(
                said.text,
                format!(
                    "run {}",
                    surco::text::one_line(&format!("{word}: {sentence}"))
                )
            );

            let waiting = WaitingOn::Run {
                reason: payload.pause.clone(),
            };
            let one_line = surco::text::one_line(&sentence);
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
        let promoted =
            yunta_replay::frame::Happening::Run(run::happening::Happening::PromotionSignaled {
                to: "ship".into(),
                cause: None,
                legacy_reason: None,
                evidence: Evidence::none(),
            });
        let said = chronicle::say(
            &yunta_replay::frame::Moment::sample(None, promoted),
            surco::glyphs::Glyphs::Unicode,
        );
        assert_eq!(said.text, "run promoted to `ship`");
    }

    /// The failure a promotion carries is read beside the node whose
    /// reroutes ran out, after the node and the mode it moves to.
    #[test]
    fn a_promotion_says_the_failure_that_exhausted_its_re_routes() {
        let cause = PromotionCause {
            node: "lint".into(),
            goto: "fix-lint".into(),
            failure: Some(Failure::Message {
                outcome: "exit 1".into(),
            }),
        };
        let promoted =
            yunta_replay::frame::Happening::Run(run::happening::Happening::PromotionSignaled {
                to: "ship".into(),
                cause: Some(cause),
                legacy_reason: None,
                evidence: Evidence::none(),
            });
        let said = chronicle::say(
            &yunta_replay::frame::Moment::sample(None, promoted),
            surco::glyphs::Glyphs::Unicode,
        );
        assert_eq!(
            said.text,
            "run promoted to `ship`: node `lint` exhausted its reroutes to `fix-lint` (exit 1)"
        );
    }
}
