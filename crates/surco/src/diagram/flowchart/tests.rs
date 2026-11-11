use crate::diagram::*;

/// Each link of `chart` as `from -> to`, with its label and stroke.
fn links(chart: &Flowchart) -> Vec<(String, String, Option<String>, Stroke)> {
    chart
        .links
        .iter()
        .map(|link| {
            (
                chart.nodes[link.from].id.clone(),
                chart.nodes[link.to].id.clone(),
                link.label.clone(),
                link.stroke,
            )
        })
        .collect()
}

fn link(
    from: &str,
    to: &str,
    label: Option<&str>,
    stroke: Stroke,
) -> (String, String, Option<String>, Stroke) {
    (
        from.to_string(),
        to.to_string(),
        label.map(str::to_string),
        stroke,
    )
}

#[test]
fn a_chart_reads_its_direction_its_boxes_and_their_labels() {
    let chart = flowchart("graph LR\n  Name[A name] --> Greeting(The greeting)").unwrap();
    assert_eq!(chart.direction, Direction::Right);
    assert_eq!(
        chart.nodes,
        [
            Node {
                id: "Name".into(),
                label: "A name".into(),
                shape: Shape::Box
            },
            Node {
                id: "Greeting".into(),
                label: "The greeting".into(),
                shape: Shape::Round
            },
        ]
    );
    assert_eq!(
        links(&chart),
        [link("Name", "Greeting", None, Stroke::Arrow)]
    );
}

#[test]
fn every_link_is_read_with_its_stroke_and_its_label() {
    let chart = flowchart(
        "flowchart TD\n a --> b\n a --- c\n a -.-> d\n a ==> e\n a -->|yes| f\n a -- no --> g\n a -. maybe .-> h\n a == sure ==> i\n a ---> j",
    )
    .unwrap();
    assert_eq!(
        links(&chart),
        [
            link("a", "b", None, Stroke::Arrow),
            link("a", "c", None, Stroke::Open),
            link("a", "d", None, Stroke::Dotted),
            link("a", "e", None, Stroke::Thick),
            link("a", "f", Some("yes"), Stroke::Arrow),
            link("a", "g", Some("no"), Stroke::Arrow),
            link("a", "h", Some("maybe"), Stroke::Dotted),
            link("a", "i", Some("sure"), Stroke::Thick),
            link("a", "j", None, Stroke::Arrow),
        ]
    );
}

#[test]
fn a_chain_and_its_groups_link_every_pair_they_join() {
    let chart = flowchart("graph TD; plan & spec --> gate --> build").unwrap();
    assert_eq!(
        links(&chart),
        [
            link("plan", "gate", None, Stroke::Arrow),
            link("spec", "gate", None, Stroke::Arrow),
            link("gate", "build", None, Stroke::Arrow),
        ]
    );
}

#[test]
fn a_question_a_quoted_label_and_its_entities_are_read_as_written() {
    let chart = flowchart(
        "graph TD\n %% the one question\n Q{\"Is it #quot;ok#quot;?<br/>Say so\"} -->|yes| R\n style Q fill:#f9f\n classDef done fill:#9f9\n R:::done",
    )
    .unwrap();
    assert_eq!(chart.nodes[0].label, "Is it \"ok\"? Say so");
    assert_eq!(chart.nodes[0].shape, Shape::Decision);
    assert_eq!(chart.nodes[1].id, "R");
    assert_eq!(links(&chart), [link("Q", "R", Some("yes"), Stroke::Arrow)]);
}

#[test]
fn an_entity_outside_quotes_ends_no_statement() {
    let chart = flowchart("graph LR; A[Say #quot;hi#quot;] --> B; B --> C").unwrap();
    assert_eq!(chart.nodes[0].label, "Say \"hi\"");
    assert_eq!(chart.links.len(), 2);
}

#[test]
fn a_node_named_again_keeps_the_label_it_was_given() {
    let chart = flowchart("graph TD\n A[Start] --> B\n B --> A").unwrap();
    assert_eq!(chart.nodes[0].label, "Start");
    assert_eq!(chart.nodes[1].label, "B");
    assert_eq!(chart.nodes.len(), 2);
}

#[test]
fn what_it_does_not_read_is_not_guessed_at() {
    for source in [
        "sequenceDiagram\n  A->>B: hi",
        "graph TD\n subgraph one\n direction LR\n A --> B\n end",
        "graph TD\n subgraph two words\n A --> B\n end",
        "graph TD\n subgraph one\n A --> B",
        "graph TD\n A --> B\n end",
        "graph TD\n subgraph A\n B\n end\n subgraph A\n C\n end",
        "graph TD\n A -> B",
        "graph XY\n A --> B",
        "graph TD",
    ] {
        assert_eq!(flowchart(source), None, "{source}");
    }
}

/// The ids of the nodes `group` holds.
fn members(chart: &Flowchart, group: &Group) -> Vec<String> {
    let id = |at: &usize| chart.nodes[*at].id.clone();
    group.members.iter().map(id).collect()
}

/// A group is a node links name by its id, holding the nodes first named
/// inside it; what is named after its `end` is not.
#[test]
fn a_group_holds_the_nodes_named_inside_it_and_links_name_it() {
    let chart = flowchart(
        "graph TD\n build[\"build\"]\n subgraph checks[\"the checks\"]\n   lint\n   test\n end\n ship\n build --> checks\n checks --> ship\n lint -.-> fix",
    )
    .unwrap();
    let [group] = chart.groups.as_slice() else {
        panic!("{:?}", chart.groups);
    };
    assert_eq!(chart.nodes[group.node].id, "checks");
    assert_eq!(chart.nodes[group.node].label, "the checks");
    assert_eq!(members(&chart, group), ["lint", "test"]);
    assert_eq!(
        links(&chart),
        [
            link("build", "checks", None, Stroke::Arrow),
            link("checks", "ship", None, Stroke::Arrow),
            link("lint", "fix", None, Stroke::Dotted),
        ]
    );
}

/// A link written inside a group joins two of its nodes; a group opened inside
/// another is one of the nodes the outer one holds.
#[test]
fn a_link_inside_a_group_and_a_group_inside_another_are_read() {
    let chart = flowchart(
        "graph LR\n subgraph outer\n   a --> b\n   subgraph inner\n     c\n   end\n end\n b --> c",
    )
    .unwrap();
    let [outer, inner] = chart.groups.as_slice() else {
        panic!("{:?}", chart.groups);
    };
    assert_eq!(members(&chart, outer), ["a", "b", "inner"]);
    assert_eq!(members(&chart, inner), ["c"]);
    assert_eq!(
        links(&chart),
        [
            link("a", "b", None, Stroke::Arrow),
            link("b", "c", None, Stroke::Arrow),
        ]
    );
}

/// A group may take the id a link named before it opened, and the label its
/// header gives.
#[test]
fn a_group_takes_the_id_a_link_named_before_it() {
    let chart = flowchart("graph TD\n build --> checks\n subgraph checks[the checks]\n lint\n end")
        .unwrap();
    let [group] = chart.groups.as_slice() else {
        panic!("{:?}", chart.groups);
    };
    assert_eq!(chart.nodes[group.node].id, "checks");
    assert_eq!(chart.nodes[group.node].label, "the checks");
    assert_eq!(members(&chart, group), ["lint"]);
    assert_eq!(
        links(&chart),
        [link("build", "checks", None, Stroke::Arrow)]
    );
}

/// A group nothing is named in is the box it names.
#[test]
fn an_empty_group_is_a_box() {
    let chart = flowchart("graph TD\n subgraph alone\n end\n alone --> next").unwrap();
    assert!(chart.groups.is_empty());
    assert_eq!(chart.nodes.len(), 2);
}
