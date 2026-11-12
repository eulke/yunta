use std::time::Duration;

use yunta_core::{NodeId, Pid};
use yunta_replay::registry::{Asking, Prompt};

use super::{asking_claim, asking_in_full, asking_on};

/// Process 4242 asking a person about `approve` at its terminal, for 3m10s.
fn approve_asked() -> Asking {
    Asking {
        pid: Pid::try_from(4242u32).expect("a process id"),
        prompt: Prompt {
            node: Some(NodeId::from("approve")),
            since: yunta_testkit_core::fixed_now(),
        },
        waited: Duration::from_secs(3 * 60 + 10),
    }
}

/// An engine asking at its terminal is said in full with how long it has
/// asked.
#[test]
fn an_engine_asking_at_its_terminal_says_how_long_it_has_asked() {
    assert_eq!(
        asking_in_full(&approve_asked()),
        "node `approve` is asking at its terminal (pid 4242) for 3m10s; answer it there"
    );
}

/// Beside a headline that says how long, or in a refusal, the claim leaves
/// the time out and keeps whose terminal it is.
#[test]
fn the_claim_of_an_engine_asking_names_its_terminal_and_not_how_long() {
    assert_eq!(
        asking_claim(&approve_asked()),
        "node `approve` is asking at its terminal (pid 4242); answer it there"
    );
}

/// A row has room for whom the engine asks about, and that the answer is
/// given at its terminal.
#[test]
fn a_row_names_whom_the_engine_asks_about() {
    assert_eq!(
        asking_on(&approve_asked()),
        "node `approve` asks at its terminal"
    );
}
