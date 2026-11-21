//! Where a run's log stops making sense, under its heading: the event replay could not take,
//! then how far the log reads, with the node the run waited on there where it waited on one.

use surco::doc::Doc;
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::{NodeId, Seq};

use super::WhereItsLogBreaks;

/// What a broken log's pane says of it, `waited_on` the node the run waited on there.
fn said(waited_on: Option<&NodeId>) -> Vec<String> {
    let blocks = WhereItsLogBreaks {
        diagnostic: "seq 3: `node_finished` is missing node_id",
        reads_up_to: Some(Seq::from(2u64)),
        waited_on,
    }
    .blocks();
    let drawn = Terminal::on(Look::plain()).draw(&Doc::new().with_all(blocks));
    drawn.lines().map(str::to_string).collect()
}

#[test]
fn a_broken_log_says_where_it_stops_and_how_far_it_reads() {
    let approve = NodeId::from("approve");
    assert_eq!(
        said(Some(&approve)),
        [
            "where its log stops making sense",
            "seq 3: `node_finished` is missing node_id",
            "it reads up to seq 2, where it waited on `approve`",
        ]
    );
    assert_eq!(
        said(None)[2],
        "it reads up to seq 2",
        "a log that waited on no node says how far it reads alone"
    );
}
