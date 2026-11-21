//! A question asked at the terminal of a run's process, as its scene says it: what is asked,
//! the process and since when in a column of labels, then that nothing here answers it.

use std::time::Duration;

use surco::doc::Doc;
use surco::look::{Look, Width};
use surco::surface::{Surface, Terminal};
use yunta_core::{NodeId, Pid};
use yunta_replay::registry::{Asking, Prompt};

use super::AskedAtItsTerminal;

/// The cells a pane is drawn in here: wide enough for every row whole.
const CELLS: usize = 84;

/// Process 4242 asking a person about `ship` at its terminal for a minute and a half, since a
/// second short of 31 minutes past the fixed instant, which reads to the minute as `00:30`.
fn ship_asked() -> Asking {
    Asking {
        pid: Pid::try_from(4242u32).expect("a process id"),
        prompt: Prompt {
            node: Some(NodeId::from("ship")),
            since: yunta_testkit_core::fixed_now() + Duration::from_secs(30 * 60 + 59),
        },
        waited: Duration::from_secs(90),
    }
}

#[test]
fn a_question_asked_elsewhere_says_who_asks_since_when_and_that_nothing_here_answers_it() {
    let look = Look {
        width: Width::exact(CELLS),
        ..Look::plain()
    };
    let asking = ship_asked();
    let asked = AskedAtItsTerminal { asking: &asking };
    assert_eq!(
        Terminal::on(look).draw(&Doc::new().with_all(asked.blocks())),
        "node `ship` asks at its terminal\npid     4242\nsince   00:30 UTC\n\n\
         nothing here answers it: answer it there\n"
    );
}
