//! The words a decision is met by, wherever a person meets it: on the
//! menu they answer at the prompt, on the page `yunta status` prints,
//! and in the trailer that closes a parked run out.
//!
//! An escalation is one object on all three, so an option is one line on
//! all three: a reader who picked `retry` off a menu and then reads the
//! same run's page has to see the same option, not two renderings of it.

use yunta_core::events::{
    EscalationError, Fact, GateOption, GateWaitingPayload, ProposedCheck, ScopeWait,
};
use yunta_core::expansion::{Precheck, Waiting};
use yunta_core::reserved::ReservedOption;
use yunta_core::text;

use crate::blocks::{departure_facts, departure_words, remedy_words, reroute_words, stuck_words};

/// What an option does, when that says more than its id: a menu that
/// prints both says one word twice. Only the label the option stores;
/// [`option_words`] writes the label an option does not store.
pub fn label(option: &GateOption) -> Option<&str> {
    option
        .label
        .as_deref()
        .filter(|label| *label != option.id.as_str())
}

/// The label and the tradeoff a person reads beside `option`, whatever the
/// option stores: the words it stores, else the words render writes for the
/// menu it is on. The label is the id when nothing else says it.
pub fn option_words(payload: &GateWaitingPayload, option: &GateOption) -> (String, Option<String>) {
    let (written_label, written_tradeoff) = menu_words(payload, option).unzip();
    let label = option
        .label
        .clone()
        .or(written_label)
        .unwrap_or_else(|| option.id.as_str().to_string());
    let tradeoff = option.tradeoff.clone().or(written_tradeoff.flatten());
    (label, tradeoff)
}

/// The words a menu writes for `option` when the option stores none: a
/// failure's, a re-route's or a departure's, and the abort every menu offers.
fn menu_words(
    payload: &GateWaitingPayload,
    option: &GateOption,
) -> Option<(String, Option<String>)> {
    let id = ReservedOption::of(&option.id)?;
    let written = match (&payload.failure, &payload.departure) {
        (Some(menu), _) => match &menu.reroute {
            Some(reroute) => reroute_words(reroute, id),
            None => remedy_words(menu, id),
        },
        (None, Some(menu)) => departure_words(menu, id),
        (None, None) => None,
    };
    written.or_else(|| abort_words(id))
}

/// The words of an abort, which pauses the run on every menu that offers it.
fn abort_words(id: ReservedOption) -> Option<(String, Option<String>)> {
    (id == ReservedOption::Abort).then(|| {
        (
            "Abort the run".to_string(),
            Some("Pauses here; nothing further executes".to_string()),
        )
    })
}

/// The lines the engine's own record is shown on, one fact to a line.
/// Empty when nothing is attached.
///
/// A menu's escalation shows the facts its menu carries first: a failure's
/// headline and detail, and the way out when it offers no remedy, or what
/// each departure said.
///
/// The record includes why a loop waits on a scope request, and what the
/// request's proposed criterion found, after the facts attached to it.
///
/// The summary is an account of what happened and this is what the
/// engine read off the log, which is why a surface puts them side by
/// side: the second is what the first is audited against. Neither one
/// repeats the other, so a surface with room for both shows both.
pub fn evidence(escalation: &GateWaitingPayload) -> Vec<String> {
    let mut lines: Vec<String> = menu_facts(escalation)
        .iter()
        .map(crate::evidence::line)
        .filter(|line| !line.is_empty())
        .collect();
    lines.extend(crate::evidence::lines(escalation.evidence()));
    if let Some(scope) = escalation.scope() {
        lines.extend(scope_facts(scope).iter().map(crate::evidence::line));
    }
    lines
}

/// The facts a menu's escalation carries beside its claim: a failure's own
/// lines and, when it offers no remedy, the way out; or a departure's facts.
fn menu_facts(escalation: &GateWaitingPayload) -> Vec<Fact> {
    match (&escalation.failure, &escalation.departure) {
        (Some(menu), _) => {
            let mut facts = crate::failure::facts(&menu.failure);
            facts.extend(
                menu.stuck
                    .as_ref()
                    .map(|stuck| Fact::labelled("way out", stuck_words(stuck))),
            );
            facts
        }
        (None, Some(menu)) => departure_facts(menu),
        (None, None) => Vec::new(),
    }
}

/// The facts a scope request is shown with: why the loop waits, and what its pre-check found.
fn scope_facts(wait: &ScopeWait) -> [Fact; 2] {
    let because = match wait.because {
        Waiting::ModeAsk => "the loop's scope mode is `ask`, so a person grants scope".to_string(),
        Waiting::SharedWithBatch => "another task of the batch may write these paths".to_string(),
        Waiting::CapExhausted { granted, cap } => format!(
            "{granted}/{cap} {} used",
            text::agreeing(cap as usize, "grant", "grants")
        ),
        Waiting::Instructions => "an instruction file, which only a person grants".to_string(),
        Waiting::Person => "a node's own request, which a person answers".to_string(),
    };
    let precheck = match &wait.proposed {
        None => "no criterion proposed".to_string(),
        Some(ProposedCheck { cmd, precheck }) => match precheck {
            Precheck::Passes => format!("`{cmd}` passes"),
            Precheck::Red { exit } => format!("`{cmd}` exits {exit}"),
            Precheck::CouldNotRun => format!("`{cmd}` could not run"),
            Precheck::NotRun => format!("`{cmd}` was not run"),
        },
    };
    [
        Fact::labelled("waiting", because),
        Fact::labelled("pre-check", precheck),
    ]
}

/// The escalation on one line, its claim and then the facts behind it,
/// for a surface with room for exactly one: the reason a run is parked
/// with, and the row a listing gives a run.
///
/// A surface with room for two parts heads each separately; a line has
/// room for neither heading. Composing them here is what keeps the page
/// and the line from disagreeing about what an escalation says.
pub fn sentence(payload: &GateWaitingPayload) -> String {
    text::aside(payload.summary(), &evidence(payload).join("; "))
}

/// The fact a refused claim repeats, on the one line the record shows it
/// on. The refusal's own sentence names no fact, so a surface that names
/// the fact adds this line to it.
pub fn repeated_fact(refused: &EscalationError) -> String {
    match refused {
        EscalationError::SummaryRepeatsEvidence { fact } => crate::evidence::line(fact),
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        CommandExit, DepartsFrom, DepartureMenu, DeviationDeclaredPayload, Escalation, Evidence,
        Fact, Failure, FailureMenu, Precondition, Remedy, Stuck,
    };
    use yunta_core::NonEmpty;

    use super::*;

    fn escalation(summary: &str, evidence: Evidence) -> GateWaitingPayload {
        // Published to a forge: the pull request is the menu, so these
        // renderings are about the claim and the record alone.
        Escalation::published_to(summary, evidence, "https://forge/pr/1")
            .expect("the summary states no fact the evidence holds")
            .into_payload()
    }

    fn option(id: &'static str, label: &str) -> GateOption {
        GateOption {
            id: yunta_core::OptionId::from_static(id),
            label: Some(label.to_string()),
            tradeoff: Some("Uses one extra correction attempt".to_string()),
            asks: None,
        }
    }

    /// An option the engine stores with no words: a menu writes them.
    fn bare(id: &str) -> GateOption {
        GateOption {
            id: id.into(),
            label: None,
            tradeoff: None,
            asks: None,
        }
    }

    /// The words of the option `id` on `payload`.
    fn words_of(payload: &GateWaitingPayload, id: &str) -> (String, Option<String>) {
        let option = payload
            .options()
            .iter()
            .find(|option| option.id.as_str() == id)
            .expect("the option is on the menu");
        option_words(payload, option)
    }

    #[test]
    fn an_option_is_labelled_only_by_what_its_id_does_not_say() {
        assert_eq!(
            label(&option("retry", "Re-route to `fix-lint` once more")),
            Some("Re-route to `fix-lint` once more")
        );
        assert_eq!(label(&option("approve", "approve")), None);
    }

    #[test]
    fn each_fact_the_engine_attached_gets_its_own_line() {
        let capped = escalation(
            "run `r` exhausted its token budget",
            vec![
                Fact::labelled("limits.max_tokens_per_run", "400"),
                Fact::labelled(
                    "total input+output tokens derived from the event log",
                    "500",
                ),
            ]
            .into(),
        );
        assert_eq!(
            evidence(&capped),
            vec![
                "limits.max_tokens_per_run: 400",
                "total input+output tokens derived from the event log: 500",
            ]
        );
    }

    #[test]
    fn a_fact_that_names_itself_is_shown_without_a_label_invented_for_it() {
        let exhausted = escalation(
            "node `lint` failed and its 0 re-routes to `fix-lint` are exhausted",
            vec![Fact::bare("exit 1")].into(),
        );
        assert_eq!(evidence(&exhausted), vec!["exit 1"]);
    }

    #[test]
    fn an_escalation_with_nothing_attached_shows_no_record() {
        assert!(evidence(&escalation("Approve the plan?", Evidence::none())).is_empty());
    }

    #[test]
    fn a_log_that_recorded_its_record_as_one_string_still_shows_it() {
        let older = escalation(
            "Approve the plan?",
            Evidence::Prose("assignee: lead".into()),
        );
        assert_eq!(evidence(&older), vec!["assignee: lead"]);
    }

    #[test]
    fn an_escalation_reads_as_its_claim_then_the_facts_behind_it() {
        let capped = escalation(
            "run `r` exhausted its token budget",
            vec![Fact::labelled("limits.max_tokens_per_run", "400")].into(),
        );
        assert_eq!(
            sentence(&capped),
            "run `r` exhausted its token budget — limits.max_tokens_per_run: 400"
        );
        assert_eq!(
            sentence(&escalation("Approve the release?", Evidence::none())),
            "Approve the release?"
        );
    }

    #[test]
    fn a_refused_claim_shows_the_fact_it_repeats_on_the_line_the_record_uses() {
        let labelled = EscalationError::SummaryRepeatsEvidence {
            fact: Fact::labelled("limits.max_tokens_per_run", "400"),
        };
        assert_eq!(repeated_fact(&labelled), "limits.max_tokens_per_run: 400");
        let bare = EscalationError::SummaryRepeatsEvidence {
            fact: Fact::bare("exit 1"),
        };
        assert_eq!(repeated_fact(&bare), "exit 1");
    }

    #[test]
    fn a_scope_gate_says_why_the_loop_waits_and_what_its_pre_check_found() {
        let payload = Escalation::published_to(
            "task `t` requests scope expansion: needs a.rs",
            vec![Fact::labelled("paths", "src/a.rs")].into(),
            "https://forge/pr/1",
        )
        .expect("the summary states no fact the evidence holds")
        .with_scope(ScopeWait {
            because: Waiting::CapExhausted { granted: 1, cap: 1 },
            proposed: Some(ProposedCheck {
                cmd: "test -f a".to_string(),
                precheck: Precheck::Red { exit: 1 },
            }),
        })
        .into_payload();
        assert_eq!(
            evidence(&payload),
            vec![
                "paths: src/a.rs",
                "waiting: 1/1 grant used",
                "pre-check: `test -f a` exits 1",
            ]
        );
        assert!(
            sentence(&payload).ends_with("waiting: 1/1 grant used; pre-check: `test -f a` exits 1")
        );
    }

    /// A failure's escalation as the engine builds it: the node's name for
    /// its claim, no record, and the menu beside it.
    fn failure_payload(menu: FailureMenu, options: Vec<GateOption>) -> GateWaitingPayload {
        let mut options = options.into_iter();
        let first = options.next().expect("a menu offers at least one option");
        let rest: Vec<GateOption> = options.collect();
        Escalation::new(
            "node `lint` failed",
            Evidence::none(),
            NonEmpty::from((first, rest)),
        )
        .expect("the summary states no fact the evidence holds")
        .with_failure(menu)
        .into_payload()
    }

    #[test]
    fn a_retry_is_worded_by_the_input_it_waits_for() {
        let menu = FailureMenu {
            attempt: 2,
            failure: Failure::Exited {
                exited: CommandExit {
                    origin: None,
                    code: 1,
                    tail: Vec::new(),
                    output: None,
                },
            },
            remedies: vec![Remedy {
                option: ReservedOption::Retry,
                precondition: Some(Precondition::OutsideTreeChanged),
            }],
            stuck: None,
            reroute: None,
        };
        let payload = failure_payload(menu, vec![bare("retry"), bare("abort")]);
        let (label, tradeoff) = words_of(&payload, "retry");
        assert_eq!(
            label,
            "Run it again (attempt 2) once something outside the tree changes"
        );
        assert!(
            tradeoff.as_deref().is_some_and(|tradeoff| tradeoff
                .ends_with("Until something outside the tree changes, it fails the same way and asks again")),
            "{}", tradeoff.as_deref().unwrap_or("none")
        );
        assert_eq!(
            words_of(&payload, "abort"),
            (
                "Abort the run".to_string(),
                Some("Pauses here; nothing further executes".to_string())
            )
        );
        assert_eq!(evidence(&payload), vec!["exit 1"]);
    }

    #[test]
    fn a_failure_with_no_remedy_says_its_way_out_beside_what_failed() {
        let menu = FailureMenu {
            attempt: 1,
            failure: Failure::PathsDenied {
                denied_paths: vec!["a.txt".into()],
                reasons: Vec::new(),
            },
            remedies: Vec::new(),
            stuck: Some(Stuck::ProjectDenies),
            reroute: None,
        };
        let payload = failure_payload(menu, vec![bare("abort")]);
        assert_eq!(
            evidence(&payload),
            vec![
                "wrote what no session of the run may write — a.txt: undo those changes",
                "way out: the project's permissions refuse it, so no attempt goes \
                 differently: change what they refuse, or the permissions themselves",
            ]
        );
    }

    #[test]
    fn a_departure_says_what_accepting_waives_and_what_the_session_said() {
        let deviation = DeviationDeclaredPayload {
            task_id: "greet".into(),
            from: DepartsFrom::Criterion("grep -q Hello greeting.txt".to_string()),
            planned: "the greeting names the person".to_string(),
            instead: "it prints a fixed greeting".to_string(),
            why: "the person is not known yet".to_string(),
        };
        let mut payload = Escalation::new(
            "task `greet`'s session departs from the plan",
            Evidence::none(),
            NonEmpty::from((bare("accept"), vec![bare("send-back")])),
        )
        .expect("the summary states no fact the evidence holds")
        .into_payload();
        payload.departure = Some(Box::new(DepartureMenu {
            deviations: vec![deviation],
            waived: vec!["grep -q Hello greeting.txt".to_string()],
            rewritten_by: None,
            accept_offered: true,
        }));
        let (label, tradeoff) = words_of(&payload, "accept");
        assert_eq!(label, "Accept the departure");
        assert!(
            tradeoff
                .as_deref()
                .is_some_and(|tradeoff| tradeoff.starts_with(
                    "The task stops being held to `grep -q Hello greeting.txt` and closes"
                )),
            "{}",
            tradeoff.as_deref().unwrap_or("none")
        );
        let record = evidence(&payload);
        assert_eq!(
            record[0],
            "departs from: the criterion `grep -q Hello greeting.txt`"
        );
        assert_eq!(record[3], "because: the person is not known yet");
    }
}
