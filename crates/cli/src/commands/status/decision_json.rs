//! A parked run's decision as a program reads it: the escalation's data under
//! the names its `gate_waiting` event writes them with, and the words a person
//! reads for its claim, its facts, its options and what it withholds.

use std::collections::BTreeMap;

use yunta_core::events::{DecisionOn, Fact, GateWaitingPayload, Menu, Shown};
use yunta_core::step::{Choice, NextStep};
use yunta_core::RunId;
use yunta_render::command;

use yunta_render::command::Reader;
use yunta_render::escalation::claim_words;

/// A parked run's decision as the versioned JSON carries it: the node it
/// belongs to, the claim and the facts a person reads beside it, every option
/// with the words it is read by, what the gate withholds and why, the menu's
/// own data, and the commands that answer it.
#[derive(serde::Serialize)]
pub(crate) struct DecisionJson {
    /// The node `resolve_gate` records the answer against. Absent for a decision
    /// that belongs to the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    node: Option<String>,
    /// The claim, in the words a person reads.
    summary: String,
    /// The facts a person reads beside the claim.
    evidence: Vec<Fact>,
    /// Every option the gate offers, in the order it offers them.
    options: Vec<OptionJson>,
    /// The forge's own handle for the gate, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    external_ref: Option<String>,
    /// The artifacts the decision is about, as the person was shown them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    shows: Vec<Shown>,
    /// The options the gate does not offer, each with why, in words.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    withheld: Vec<WithheldJson>,
    /// What the escalation is about, under the names `gate_waiting` writes.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    menu: Option<Menu>,
    /// The command a person runs to answer, with `<option>` left for one
    /// of the ids above it.
    resolve_with: String,
    /// The command that chooses each option, by its id.
    commands: BTreeMap<String, String>,
}

/// One option as a program reads it: its id, and the words a person reads
/// beside it.
#[derive(serde::Serialize)]
struct OptionJson {
    id: String,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tradeoff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asks: Option<String>,
}

/// An option the gate withholds, and why, in the words a person reads.
#[derive(serde::Serialize)]
struct WithheldJson {
    option: String,
    because: String,
}

impl DecisionJson {
    pub(crate) fn new(run_id: &RunId, on: &DecisionOn, escalation: GateWaitingPayload) -> Self {
        DecisionJson {
            node: on.node().map(ToString::to_string),
            summary: claim_words(&escalation),
            evidence: yunta_render::escalation::facts(&escalation),
            options: options_of(&escalation),
            external_ref: escalation.external_ref().map(str::to_string),
            shows: escalation.shows().to_vec(),
            withheld: withheld_of(&escalation),
            menu: escalation.menu().cloned(),
            resolve_with: command::line_for(
                &NextStep::ResolveGate {
                    run: run_id.clone(),
                    choice: Choice::Blank,
                },
                Reader::Agent,
            ),
            commands: commands_of(run_id, &escalation),
        }
    }
}

/// Every option the gate offers, with the words a person reads beside it.
fn options_of(escalation: &GateWaitingPayload) -> Vec<OptionJson> {
    yunta_render::offers::decision_options(escalation)
        .into_iter()
        .map(|option| OptionJson {
            id: option.id,
            label: option.label.unwrap_or_default(),
            tradeoff: option.tradeoff,
            asks: option.asks,
        })
        .collect()
}

/// The options the gate does not offer, each with why, in words.
fn withheld_of(escalation: &GateWaitingPayload) -> Vec<WithheldJson> {
    escalation
        .withheld()
        .iter()
        .map(|withheld| WithheldJson {
            option: withheld.option.to_string(),
            because: yunta_render::escalation::because(withheld),
        })
        .collect()
}

/// The command that chooses each option, by its id. An option that asks for
/// words carries the answer's flag: a program that runs it as written is not
/// refused for want of them.
fn commands_of(run_id: &RunId, escalation: &GateWaitingPayload) -> BTreeMap<String, String> {
    escalation
        .options()
        .iter()
        .map(|option| {
            let step = NextStep::ResolveGate {
                run: run_id.clone(),
                choice: Choice::Picked {
                    option: option.id.clone(),
                    words: option.asks().is_some(),
                },
            };
            (
                option.id.to_string(),
                yunta_render::command::line_for(&step, Reader::Agent),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use yunta_core::events::{
        AuthorGate, Claim, Escalation, GateOption, Leads, WhyWithheld, Withheld,
    };
    use yunta_core::reserved::ReservedOption;
    use yunta_core::shown::Flaw;
    use yunta_core::{NodeId, NonEmpty, OptionId, TaskId};

    use super::*;

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const NODE: NodeId = NodeId::from_static("approve-plan");

    /// A gate an author declared: `proceed` takes the run on to `ship`, and
    /// `revise` sends the work back to `prompt`. `approve` is offered and then
    /// withheld, because the plan it would approve cannot be proven.
    fn gate() -> GateWaitingPayload {
        let proceed = GateOption::leading(
            OptionId::from_static("proceed"),
            Leads::On {
                next: vec![NodeId::from_static("ship")],
                settles: false,
            },
        );
        let revise = GateOption::leading(
            OptionId::from_static("revise"),
            Leads::Back {
                node: NodeId::from_static("prompt"),
                resumes: true,
            },
        );
        let approve = GateOption::leading(
            OptionId::from_static("approve"),
            Leads::On {
                next: Vec::new(),
                settles: false,
            },
        );
        Escalation::new(
            Claim::NeedsDecision { node: NODE },
            Menu::Gate(AuthorGate {
                assignee: "lead".to_string(),
                node: None,
            }),
            NonEmpty::from((
                proceed,
                vec![approve, revise, GateOption::reserved(ReservedOption::Abort)],
            )),
        )
        .withholding(vec![Withheld {
            option: OptionId::from_static("approve"),
            because: WhyWithheld::Unprovable {
                flaws: NonEmpty::from((
                    Flaw::HollowSpecTest {
                        task: TaskId::from_static("T001"),
                        cmd: "test -f a.txt".to_string(),
                    },
                    Vec::new(),
                )),
            },
        }])
        .into_payload()
    }

    /// A decision the run owns has no node to record its answer against, and a reader
    /// tells that from the field being absent.
    #[test]
    fn a_decision_the_run_owns_names_no_node() {
        let owned =
            serde_json::to_value(DecisionJson::new(&RUN, &DecisionOn::Run, gate())).unwrap();
        assert!(owned.get("node").is_none(), "{owned:#}");
        let on_a_node =
            serde_json::to_value(DecisionJson::new(&RUN, &DecisionOn::Node(NODE), gate())).unwrap();
        assert_eq!(on_a_node["node"], "approve-plan", "{on_a_node:#}");
    }

    #[test]
    fn a_decision_reads_its_words_where_a_program_reads_them() {
        let json =
            serde_json::to_value(DecisionJson::new(&RUN, &DecisionOn::Node(NODE), gate())).unwrap();
        assert_eq!(json["options"][0]["label"], "On to `ship`", "{json:#}");
        assert_eq!(
            json["options"][1]["asks"], "what should change?",
            "{json:#}"
        );
        assert_eq!(
            json["commands"]["revise"],
            "yunta resolve-gate 01JBZ5X8K3N7Q2W6E4R9T1Y0P5 revise --text \"<answer>\"",
            "{json:#}"
        );
        assert_eq!(
            json["evidence"][0],
            json!({ "label": "assignee", "value": "lead" }),
            "{json:#}"
        );
        assert!(
            json["evidence"]
                .as_array()
                .is_some_and(|facts| facts.iter().any(|fact| fact["label"] == "withheld")),
            "{json:#}"
        );
        assert!(
            json["withheld"][0]["because"].as_str().is_some_and(
                |because| because.starts_with("the plan cannot be proven as it is written")
            ),
            "{json:#}"
        );
        assert_eq!(json["gate"], json!({ "assignee": "lead" }), "{json:#}");
    }

    #[test]
    fn every_option_has_the_command_that_chooses_it() {
        let json =
            serde_json::to_value(DecisionJson::new(&RUN, &DecisionOn::Node(NODE), gate())).unwrap();
        assert_eq!(
            json["commands"]["proceed"],
            "yunta resolve-gate 01JBZ5X8K3N7Q2W6E4R9T1Y0P5 proceed"
        );
        assert_eq!(
            json["commands"]["abort"],
            "yunta resolve-gate 01JBZ5X8K3N7Q2W6E4R9T1Y0P5 abort"
        );
    }

    #[test]
    fn an_engine_claim_is_said_in_the_words_a_person_reads() {
        let json =
            serde_json::to_value(DecisionJson::new(&RUN, &DecisionOn::Node(NODE), gate())).unwrap();
        assert_eq!(json["summary"], "gate `approve-plan` needs a decision");
    }
}
