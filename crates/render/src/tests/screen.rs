//! What a screen says that is no stage's page: how much room the application needs on a
//! terminal too small for it, the keys each of a run's scenes names in its legend and
//! the pair every screen answers to, and the note an act leaves or an answer given elsewhere
//! does. What help lists is `help`'s; what a confirmation asks, `confirming`'s.

use surco::components::Legend;
use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::line::Line;
use surco::look::{Look, Width};
use surco::surface::Terminal;
use yunta_core::events::{
    AuthorGate, Channel, Claim, Escalation, EventBody, EventPayload, FailureMenu, GateEvent,
    GateOption, GateWaitingPayload, Leads, Menu, QuestionsAnsweredPayload, ScopeWait, StoredEvent,
};
use yunta_core::expansion::Waiting;
use yunta_core::reserved::ReservedOption;
use yunta_core::{ContentHash, NodeId, NonEmpty, OptionId, Responder};
use yunta_replay::frame::Effect;

use crate::prompt::card::{self, ChosenWith};
use crate::prompt::{choice_list, confirming_legend, desk, round, Escape, PromptAct};
use crate::screen::{
    answers_recorded, anywhere, field_place, in_the_legend, recorded, room_needed,
    round_answered_elsewhere, run_legend, unread_run_legend, Along, Leaving, RunKeys,
    NOTHING_RECORDED, NOTHING_TO_RESUME,
};
use yunta_core::ahead::NodesAhead;

/// A terminal a column short is told the room it needs and the room it has, and how to
/// leave.
#[test]
fn a_terminal_too_small_is_told_the_room_it_needs_and_has() {
    assert_eq!(
        room_needed((60, 14), (59, 14)),
        "the application needs a terminal of 60 by 14 at least, and this one is 59 by 14: \
         make it larger, or press q to leave"
    );
}

/// `legend` as a terminal's last row spells it in Unicode.
pub(crate) fn spelled(legend: &Legend) -> String {
    spelled_in(legend, Glyphs::Unicode)
}

/// `legend` as a terminal folds it in `glyphs`, on rows wide enough to hold it on one.
pub(crate) fn spelled_in(legend: &Legend, glyphs: Glyphs) -> String {
    let look = Look {
        glyphs,
        width: Width::exact(usize::MAX),
        ..Look::plain()
    };
    let rows = Terminal::on(look).block(&Block::Legend(legend.clone()));
    rows.iter().map(Line::text).collect::<Vec<_>>().join("\n")
}

/// What a scene that moves along `along` offers, with `acts` and `new_run`, where `q`
/// goes to the field leaving the run as `leaving` says.
fn scene<'a>(
    along: Along,
    acts: &'a [(char, &'a str)],
    new_run: bool,
    leaving: Leaving,
) -> RunKeys<'a> {
    RunKeys {
        along,
        acts,
        new_run,
        leaving,
    }
}

/// A run at work that leads its screen names its nodes, its numbers, its graph and the
/// field it keeps going without, as the design's row says.
#[test]
fn a_run_at_work_while_it_leads_reads_the_designs_legend() {
    let work = scene(Along::Nodes, &[], false, Leaving::KeepsGoing);
    assert_eq!(
        spelled(&run_legend(&work, true)),
        "j k node · s stats · g graph · q field, it keeps going"
    );
}

/// A run the person took the lead of offers it back, and no graph; the findings name
/// what `j k` moves along and the key that hands it to the nodes.
#[test]
fn a_run_not_leading_offers_following_it_and_the_findings_name_their_pointer() {
    let node = scene(Along::Nodes, &[], false, Leaving::KeepsGoing);
    assert_eq!(
        spelled(&run_legend(&node, false)),
        "j k node · f follow it · s stats · q field, it keeps going"
    );
    let findings = scene(Along::Findings, &[], false, Leaving::KeepsGoing);
    assert_eq!(
        spelled(&run_legend(&findings, true)),
        "j k finding · tab nodes · s stats · q field, it keeps going"
    );
}

/// A scene's acts stand between its pointer's keys and its numbers, each in the legend's
/// words; a run that ended offers a new one; where no engine drives it, `q` goes to the
/// field with nothing said of it going on.
#[test]
fn a_scenes_acts_stand_in_the_legends_words() {
    let continues = in_the_legend(Effect::ContinuesFromLog);
    let acts = [('r', continues), ('x', in_the_legend(Effect::Closes))];
    let stalled = scene(Along::Nodes, &acts, false, Leaving::Waits);
    assert_eq!(
        spelled(&run_legend(&stalled, true)),
        "j k node · r continue it from its log · x close · s stats · g graph · q field"
    );
    let receipt = scene(Along::Nodes, &[], true, Leaving::Ended);
    assert_eq!(
        spelled(&run_legend(&receipt, true)),
        "j k node · s stats · g graph · n new run · q field"
    );
    let standstill = [('r', in_the_legend(Effect::HandsBackOnceSettled))];
    let standstill = scene(Along::Nodes, &standstill, false, Leaving::Waits);
    assert!(spelled(&run_legend(&standstill, true)).contains("r hand it back"));
}

/// Before the run is read, `q` goes to the field with nothing said of the run, and no key
/// moves along nodes not read yet.
#[test]
fn a_run_not_read_yet_offers_the_field_alone() {
    assert_eq!(spelled(&unread_run_legend()), "q field");
}

/// The field stands on the trail as the repository it lists, its path under the home mark
/// where it is under the home, or as the machine where it lists every run on it.
#[test]
fn the_field_stands_on_the_trail_as_its_repository_or_the_machine() {
    let home = std::path::Path::new("/home/me");
    let repository = std::path::Path::new("/home/me/development/yunta");
    assert_eq!(
        field_place(Some(repository), Some(home)),
        "~/development/yunta"
    );
    assert_eq!(
        field_place(Some(std::path::Path::new("/cut/worktree")), Some(home)),
        "/cut/worktree"
    );
    assert_eq!(field_place(None, Some(home)), "this machine");
}

/// What `r` on the field leaves noted where the run pointed at waits on a decision, which
/// no resume answers.
#[test]
fn resuming_a_run_that_waits_on_a_decision_is_noted_as_the_design_says() {
    assert_eq!(
        NOTHING_TO_RESUME,
        "Nothing to resume: this one waits on a decision. Enter reads it."
    );
}

/// The keys a person presses anywhere they are not typing: the palette and help.
#[test]
fn anywhere_holds_the_palette_and_help_in_the_designs_words() {
    assert_eq!(spelled(&anywhere()), "/ anything · ? keys");
}

/// A decision's prompts on a run's screen, in the words their screens say: a card with and
/// without the plan, its words, a round in its turns, the desk and the desk confirming.
#[test]
fn a_decisions_prompts_name_their_keys_as_the_design_does() {
    let (waiting, later) = (
        PromptAct::LeaveItWaiting.said(),
        PromptAct::LeaveItForLater.said(),
    );
    let letters = ChosenWith::Letters;
    let legends = [
        (
            card::choosing_legend(letters, false, waiting),
            "j k choose · enter answer · n add a note · esc leave it waiting",
        ),
        (
            card::choosing_legend(letters, true, waiting),
            "j k choose · enter answer · n add a note · r read the plan · esc leave it waiting",
        ),
        (card::saying_legend(false), "enter record it · esc go back"),
        (card::saying_legend(true), "enter send it · esc go back"),
        (
            round::legend(true, true, later),
            "↑↓ choose · enter answer · shift-tab go back · esc leave it for later",
        ),
        (
            round::legend(false, false, later),
            "enter answer · esc leave it for later",
        ),
        (confirming_legend(), "enter confirm · esc go back"),
        (
            desk::legend(3, true, true, waiting),
            "1 2 3 answer · tab next place · ↑↓ scroll · p proof · d decided for you · esc \
             leave it waiting",
        ),
    ];
    for (legend, said) in legends {
        assert_eq!(spelled(&legend), said);
    }
}

/// What each key of `legend` does, in order, its keys left out: a prompt at a terminal may
/// take other keys than a screen does, never other words for one act.
fn acts(legend: &Legend) -> Vec<&str> {
    legend
        .0
        .iter()
        .map(|binding| binding.does.as_str())
        .collect()
}

/// A prompt at a terminal names each act in the words of the screen that asks the same
/// thing, the arrows choosing on its card where the screen's card takes `j k`; Escape says
/// what it does there as an order, a setup's as its screen says it.
#[test]
fn a_prompt_at_a_terminal_names_each_act_in_the_screens_words() {
    let waiting = PromptAct::LeaveItWaiting.said();
    let at_a_terminal =
        card::choosing_legend(ChosenWith::Arrows, true, Escape::LeavesWaiting.does());
    let on_a_screen = card::choosing_legend(ChosenWith::Letters, true, waiting);
    assert_eq!(acts(&at_a_terminal), acts(&on_a_screen));
    assert_eq!(
        spelled(&at_a_terminal),
        "↑↓ choose · enter answer · n add a note · r read the plan · esc leave it waiting"
    );
    let parked = round::legend(true, true, Escape::Parks.does());
    let put_aside = round::legend(true, true, PromptAct::LeaveItForLater.said());
    assert_eq!(acts(&parked), acts(&put_aside));
    let list = choice_list(None, None, Escape::TakesTheFirst).legend;
    assert_eq!(
        spelled(&list),
        "↑↓ choose · enter answer · esc take the first"
    );
    let escapes = [
        (Escape::GoesBack, "go back"),
        (Escape::KeepsDefault, "keep the detected default"),
    ];
    for (escape, does) in escapes {
        assert_eq!(escape.does(), does);
    }
}

/// A failed node's menu offering a retry, and a scope request's offering a grant.
fn asked(menu: Menu, option: ReservedOption) -> GateWaitingPayload {
    let claim = Claim::NeedsDecision {
        node: "work".into(),
    };
    let options = NonEmpty::from((GateOption::reserved(option), Vec::new()));
    Escalation::new(claim, menu, options).into_payload()
}

/// An answer recorded says the option it recorded and what the run does now, by the
/// kind of option chosen.
#[test]
fn the_sentence_recorded_for_an_answer_is_the_designs() {
    let ahead = NodesAhead::default();
    let scope = Menu::Scope(ScopeWait {
        because: Waiting::ModeAsk,
        paths: vec!["b.txt".into()],
        proposed: None,
    });
    let grant = asked(scope, ReservedOption::Grant);
    assert_eq!(
        recorded(&grant, &ReservedOption::Grant.id(), None, &ahead),
        "Recorded: grant. The session goes on with the wider scope."
    );
    let failed = Menu::Failure(Box::new(FailureMenu {
        attempt: 1,
        failure: yunta_core::events::Failure::Message {
            outcome: "exit 1".into(),
        },
        remedies: Vec::new(),
        stuck: None,
        reroute: None,
    }));
    let retry = asked(failed, ReservedOption::Retry);
    assert_eq!(
        recorded(&retry, &ReservedOption::Retry.id(), None, &ahead),
        "Recorded: retry. A fresh attempt starts from the run's tree."
    );
    // A loop that ended owing its task's request picks the task's work back up in the
    // session it worked in last.
    let owed = Menu::Failure(Box::new(FailureMenu {
        attempt: 1,
        failure: yunta_core::events::Failure::ScopeOwed {
            owed: vec![yunta_core::events::OwedScope {
                task_id: "greet".into(),
                paths: vec!["b.txt".into()],
                reason: None,
            }],
        },
        remedies: Vec::new(),
        stuck: None,
        reroute: None,
    }));
    let grant = asked(owed, ReservedOption::Grant);
    assert_eq!(
        recorded(&grant, &ReservedOption::Grant.id(), None, &ahead),
        "Recorded: grant. The session goes on with the wider scope."
    );
}

/// An author's option that goes on names the node it goes on to.
#[test]
fn an_option_that_goes_on_is_recorded_with_where_it_goes() {
    let approve = OptionId::from_static("approve");
    let on = Leads::On {
        next: vec![NodeId::from_static("pr")],
        settles: false,
    };
    let gate = Menu::Gate(AuthorGate {
        assignee: "lead".into(),
        node: None,
    });
    let claim = Claim::NeedsDecision {
        node: "ship".into(),
    };
    let options = NonEmpty::from((GateOption::leading(approve.clone(), on), Vec::new()));
    let payload = Escalation::new(claim, gate, options).into_payload();
    assert_eq!(
        recorded(&payload, &approve, None, &NodesAhead::default()),
        "Recorded: approve. On to `pr`."
    );
    let left = "Recorded: approve. The gate finishes; `pr` is not in this run.";
    assert_eq!(recorded(&payload, &approve, None, &pr_left_out()), left);
}

/// An author's option that goes back says where the words said with it went, the design's
/// sentence; with no words it says where the run goes back to.
#[test]
fn an_option_sent_back_with_words_is_noted_with_them() {
    let adjust = OptionId::from_static("adjust");
    let back = Leads::Back {
        node: NodeId::from_static("plan"),
        resumes: true,
    };
    let gate = Menu::Gate(AuthorGate {
        assignee: "lead".into(),
        node: None,
    });
    let claim = Claim::NeedsDecision {
        node: "approve-plan".into(),
    };
    let options = NonEmpty::from((GateOption::leading(adjust.clone(), back), Vec::new()));
    let payload = Escalation::new(claim, gate, options).into_payload();
    let ahead = NodesAhead::default();
    assert_eq!(
        recorded(&payload, &adjust, Some("split the second task"), &ahead),
        "Sent back to `plan` with your words: \"split the second task\"."
    );
    assert_eq!(
        recorded(&payload, &adjust, None, &ahead),
        "Recorded: adjust. Back to `plan`; the gate asks again once it has run."
    );
}

/// What lies ahead at the nodes of a run whose config declares no forge, which leaves its
/// optional `pr` out.
fn pr_left_out() -> NodesAhead {
    let workflow = "name: w\nnodes:\n  - { id: ship, kind: gate, assignee: lead, message: Ship?, \
                    options: [approve] }\n  - { id: pr, kind: pull_request, title: it, optional: \
                    true, depends_on: [ship] }\n";
    let workflow: yunta_core::Workflow =
        yunta_core::workflow::read::read(workflow, std::path::Path::new("w.yaml")).unwrap();
    let config: yunta_core::ConfigLayer = yunta_core::yaml::parse("{}\n").unwrap();
    let left = yunta_core::left_out(&workflow, &config);
    NodesAhead::of(&workflow, &config, &left)
}

/// A round put aside records nothing, and leaving a run says whether it keeps going.
#[test]
fn a_round_put_aside_and_a_run_left_each_say_what_holds() {
    assert_eq!(
        NOTHING_RECORDED,
        "Nothing recorded. The run stays waiting on these questions."
    );
    assert_eq!(
        Leaving::KeepsGoing.said(),
        "The run keeps going: no engine lives in this window."
    );
    let said = [Leaving::KeepsGoing, Leaving::Waits, Leaving::Ended].map(Leaving::said);
    assert!(said[1] != said[0] && said[2] != said[1], "{said:?}");
}

/// A round's answers recorded say how many, and that the node that asked finishes with them
/// and the run goes on; a round answered with none says the node goes on with what each
/// question assumes.
#[test]
fn a_rounds_answers_recorded_say_how_many_and_what_the_run_does_with_them() {
    let grill = NodeId::from("grill");
    assert_eq!(
        answers_recorded(&grill, 3),
        "Recorded: 3 answers. `grill` finishes with them, and the run goes on."
    );
    assert_eq!(
        answers_recorded(&grill, 1),
        "Recorded: 1 answer. `grill` finishes with it, and the run goes on."
    );
    assert_eq!(
        answers_recorded(&grill, 0),
        "Recorded: no answer. `grill` finishes with what each question assumes, and the run \
         goes on."
    );
}

/// A round answered elsewhere is noted with who answered it and through what, in the words its
/// chronicle's row says after its subject and `answered`: by its responder where the log names
/// one, via the channel as the log spells it. The row says each as it always did.
#[test]
fn a_round_answered_elsewhere_is_noted_in_its_chronicle_rows_words() {
    let lead: Responder = "lead".parse().expect("a responder's name");
    let cases = [
        (
            Channel::Mcp,
            Some(lead),
            "Answered elsewhere: by lead via mcp.",
            "run answered by lead via mcp",
        ),
        (
            Channel::Tty,
            None,
            "Answered elsewhere: via tty.",
            "run answered via tty",
        ),
    ];
    for (channel, responder, noted, row) in cases {
        let answered = QuestionsAnsweredPayload {
            answers_hash: ContentHash::sha256(b"answers"),
            channel,
            responder,
        };
        assert_eq!(round_answered_elsewhere(&answered), noted);
        let payload = EventPayload::Gates(GateEvent::QuestionsAnswered(answered));
        assert_eq!(chronicle_row(payload), row);
    }
}

/// What the chronicle says of one event of the run, its subject first, as every surface
/// derives it from the log.
fn chronicle_row(payload: EventPayload) -> String {
    let events = vec![StoredEvent {
        seq: 1.into(),
        run_id: "01JQ0000000000000000000000".into(),
        node_id: None,
        timestamp: chrono::DateTime::UNIX_EPOCH,
        body: EventBody::Known(payload),
    }];
    let moments = yunta_replay::frame::chronicle(&events);
    let moment = moments.first().expect("one moment per event");
    crate::chronicle::say(moment, Glyphs::Unicode).text
}

/// A legend says each act as an order, in the words `PromptAct` owns: stopping the cases, the
/// sessions and a collection under way, leaving where nothing else is left, taking the
/// palette's row; and a pack's removal as its confirmation names it.
#[test]
fn legends_say_their_acts_as_orders() {
    use crate::prompt::PromptAct;
    use crate::screen::{
        cases_legend, doctor_legend, leaving_legend, packs_legend, palette_legend, upkeep_legend,
        DoctorChecks,
    };

    assert_eq!(spelled(&cases_legend()), "esc stop them");
    let sessions = doctor_legend(false, DoctorChecks::OpeningSessions);
    assert_eq!(spelled(&sessions), "esc stop the sessions");
    assert_eq!(spelled(&upkeep_legend(true, true)), "esc stop it");
    assert_eq!(spelled(&leaving_legend()), "q quit");
    assert!(spelled(&palette_legend()).contains("enter do it"));
    let removes = PromptAct::RemoveIt.said();
    assert!(spelled(&packs_legend()).contains(&format!("x {removes}")));
}
