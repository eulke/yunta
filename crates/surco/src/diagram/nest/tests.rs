use super::{Ends, Nest};
use crate::diagram::flowchart;

/// Each label of `nest`'s chart, in its order.
fn labels(nest: &Nest) -> Vec<&str> {
    nest.chart
        .nodes
        .iter()
        .map(|node| node.label.as_str())
        .collect()
}

/// Each link of `nest`'s chart, by its labels.
fn links(nest: &Nest) -> Vec<(&str, &str)> {
    let label = |at: usize| nest.chart.nodes[at].label.as_str();
    nest.chart
        .links
        .iter()
        .map(|link| (label(link.from), label(link.to)))
        .collect()
}

/// Outside the group, it is one node: the links to and from it are the outside
/// chart's, and a link from a node it holds leaves the group from that node.
#[test]
fn a_group_is_one_node_outside_and_its_chart_inside() {
    let chart = flowchart(
        "graph TD\n build\n subgraph checks\n lint\n test\n end\n fix\n ship\n build --> checks\n checks --> ship\n lint -.-> fix",
    )
    .unwrap();
    let nest = Nest::of(&chart).unwrap();
    assert_eq!(labels(&nest), ["build", "checks", "fix", "ship"]);
    assert_eq!(
        links(&nest),
        [("build", "checks"), ("checks", "ship"), ("checks", "fix")]
    );
    assert_eq!(
        nest.ends,
        [
            Ends::default(),
            Ends::default(),
            Ends {
                from: Some(0),
                to: None
            }
        ]
    );
    let inner = nest.inner[1].as_ref().expect("the group's chart");
    assert_eq!(labels(inner), ["lint", "test"]);
    assert!(inner.chart.links.is_empty());
    assert!(nest.inner[0].is_none() && nest.inner[2].is_none());
}

/// A link between two nodes of one group is the group's; a group held in another
/// is a node of the other's chart, and a link reaching a node inside it reaches
/// the inner group from the outer one.
#[test]
fn a_link_inside_a_group_and_a_group_inside_another_are_taken_apart_in_turn() {
    let chart = flowchart(
        "graph LR\n start\n subgraph outer\n a --> b\n subgraph inner\n c\n end\n end\n start --> c\n b --> c",
    )
    .unwrap();
    let nest = Nest::of(&chart).unwrap();
    assert_eq!(labels(&nest), ["start", "outer"]);
    assert_eq!(links(&nest), [("start", "outer")]);
    assert_eq!(
        nest.ends,
        [Ends {
            from: None,
            to: Some(2)
        }]
    );
    let outer = nest.inner[1].as_ref().expect("the outer group's chart");
    assert_eq!(labels(outer), ["a", "b", "inner"]);
    assert_eq!(links(outer), [("a", "b"), ("b", "inner")]);
    let inner = outer.inner[2].as_ref().expect("the inner group's chart");
    assert_eq!(labels(inner), ["c"]);
}

/// A link between a group and a node it holds is not taken apart.
#[test]
fn a_link_from_a_group_to_what_it_holds_is_not_taken_apart() {
    let chart = flowchart("graph TD\n subgraph g\n a\n end\n g --> a").unwrap();
    assert_eq!(Nest::of(&chart), None);
}
