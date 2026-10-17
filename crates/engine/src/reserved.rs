//! The escalation options the engine builds. The reserved vocabulary is one
//! closed enum in the core crate, built from here and read back through it,
//! so no run-loop branch matches a human's choice or a node's outcome against
//! a bare string literal.

use yunta_core::events::{Decider, HumanChoice};
use yunta_core::expansion::{node_offer, Decision, Denial};
use yunta_core::protected::Protected;
use yunta_core::reserved::ReservedOption;
use yunta_core::ScopeGlob;

/// What a person's choice decides about a request the loop escalated: the
/// grant the choice names, unless the protected set refuses one of its paths,
/// or else the refusal the choice is. A task's own request is always
/// widenable.
pub(crate) fn decision_of_choice(
    choice: &HumanChoice,
    paths: &[ScopeGlob],
    protected: &Protected,
    by: &Decider,
) -> Decision {
    if ReservedOption::of(&choice.option) != Some(ReservedOption::Grant) {
        return Decision::Deny(Denial::Chosen {
            option: choice.option.as_str().to_string(),
            note: choice.free_text.clone(),
        });
    }
    match node_offer(paths, protected, true) {
        Ok(granted) => Decision::Grant {
            paths: granted,
            by: by.clone(),
        },
        Err(denial) => Decision::Deny(denial),
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{GateOption, Leads};
    use yunta_core::reserved::ReservedOption;
    use yunta_core::OptionId;

    /// Every option the engine offers: each reserved option, built the way a
    /// menu builds it, and an author's option that goes on past a gate and
    /// keeps the id it declares. The closed set an escalation draws from.
    fn every_offer() -> Vec<GateOption> {
        let on = Leads::On {
            next: Vec::new(),
            settles: false,
        };
        ReservedOption::all()
            .into_iter()
            .map(GateOption::reserved)
            .chain([GateOption::leading(OptionId::from_static("ship-it"), on)])
            .collect()
    }

    #[test]
    fn an_offer_is_answered_by_the_id_the_engine_reads_back() {
        for option in every_offer() {
            let id = &option.id;
            assert!(
                ReservedOption::of(id).is_some() || id.as_str() == "ship-it",
                "`{id}` is offered but no reserved option spells it, so the run \
                 loop cannot act on the answer"
            );
        }
    }
}
