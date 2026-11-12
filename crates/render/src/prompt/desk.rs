//! What a plan's desk says around what is read: its title, what stands at its far
//! edge, and the keys that do something from where it stands.

use surco::components::Legend;
use surco::words::Standing;

use super::{Escape, ESC};
use crate::page::desk::View;

/// Where a desk stands, as the keys that do something there depend on it.
pub struct DeskKeys {
    /// Whether an option a number named waits to be confirmed.
    pub confirming: bool,
    /// What the pane shows of the place stood on.
    pub view: View,
    /// Whether the plan offers choices to read, where the pane shows them.
    pub has_choices: bool,
    /// Whether the place stood on has a proof to turn to.
    pub has_proof: bool,
    /// Whether the plan says what it decided for the person.
    pub says_decided: bool,
    /// Where Escape leaves to: back to the card it opened from, or out of the command
    /// with what that Escape does.
    pub leaving: Option<Escape>,
}

/// The title a desk is headed with: the plan, then the run's handle and the workflow it
/// follows, each when there is one.
pub fn title(run: Option<String>, called: Option<String>) -> Vec<String> {
    std::iter::once("plan".to_string())
        .chain(run)
        .chain(called)
        .collect()
}

/// What stands on the desk's far edge: what needs the person, and the node it is on.
pub fn edge(node: Option<&str>) -> String {
    let needs = Standing::NeedsYou.word();
    match node {
        Some(node) => format!("{needs}  {node}"),
        None => needs.to_string(),
    }
}

/// The keys that do something from where the desk stands, each with what it does.
pub fn legend(keys: &DeskKeys) -> Legend {
    if keys.confirming {
        return Legend::new().key("enter", "confirms").key(ESC, "goes back");
    }
    let legend = match keys.view {
        View::Choices if !keys.has_choices => Legend::new(),
        View::Choices => Legend::new().key("j k", "choice"),
        _ => Legend::new().key("j k", "task"),
    }
    .arrows("scroll");
    let legend = match (keys.has_proof, keys.view) {
        (false, _) => legend,
        (true, View::Proof) => legend.key("p", "the card"),
        (true, _) => legend.key("p", "proof"),
    };
    let legend = match (keys.says_decided, keys.view) {
        (false, _) => legend,
        (true, View::Choices) => legend.key("d", "the tasks"),
        (true, _) => legend.key("d", "decided for you"),
    };
    let leaving = match keys.leaving {
        None => "back",
        Some(escape) => escape.does(),
    };
    legend.key(ESC, leaving)
}
