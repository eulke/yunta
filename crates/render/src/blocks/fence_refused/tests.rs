//! The writes the fence refused, as a scope's card rests on them: each path the request asks
//! for that the fence refused, once, under the failed mark; none it does not ask for.

use surco::doc::Doc;
use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::events::{
    Claim, Escalation, Failure, FailureMenu, GateOption, GateWaitingPayload, Menu, OwedScope,
    RefusedWrite, ToolTarget,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, NonEmpty, ScopeGlob, SessionId, TaskId};

use super::FenceRefused;

/// A loop that failed owing a person its task's request to write `path`.
fn owing(path: &str) -> GateWaitingPayload {
    let failure = Failure::ScopeOwed {
        owed: vec![OwedScope {
            task_id: TaskId::from("greet"),
            paths: vec![ScopeGlob::from(path)],
            reason: None,
        }],
    };
    let menu = FailureMenu {
        attempt: 1,
        failure,
        remedies: Vec::new(),
        stuck: None,
        reroute: None,
    };
    let options = NonEmpty::from((
        GateOption::reserved(ReservedOption::Grant),
        vec![GateOption::reserved(ReservedOption::Deny)],
    ));
    let node = NodeId::from_static("implement");
    Escalation::new(
        Claim::NodeFailed { node },
        Menu::Failure(Box::new(menu)),
        options,
    )
    .into_payload()
}

/// A write of `path` the fence refused a session.
fn refused(path: &str) -> RefusedWrite {
    RefusedWrite {
        session_id: SessionId::from("session-1"),
        target: ToolTarget::named(path),
    }
}

#[test]
fn each_refused_write_the_request_asks_for_is_said_once_and_no_other() {
    let writes = [
        refused("src/b.rs"),
        refused("src/a.rs"),
        refused("src/a.rs"),
    ];
    let fence = FenceRefused::asked_by(&owing("src/a.rs"), &writes);
    let doc = Doc::new().with_all(fence.blocks(Glyphs::Unicode));
    let drawn = Terminal::on(Look::plain()).draw(&doc);
    assert_eq!(
        drawn.lines().collect::<Vec<_>>(),
        ["✗ the fence refused a write: src/a.rs"]
    );
}

#[test]
fn a_request_no_refused_write_led_to_rests_on_nothing_of_the_fence() {
    let fence = FenceRefused::asked_by(&owing("src/a.rs"), &[refused("src/b.rs")]);
    assert!(fence.blocks(Glyphs::Unicode).is_empty());
}
