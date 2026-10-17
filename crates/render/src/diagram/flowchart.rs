//! A Mermaid flowchart read into a graph: which way it reads, the boxes
//! it names and the links between them. What this does not read — a
//! subgraph, any other kind of diagram, a line it cannot parse — is not
//! guessed at: the source is shown as written instead.

/// A flowchart: the way it reads, its nodes in the order the source
/// first names them, and its links in the order it draws them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flowchart {
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
}

/// The way a flowchart reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Top to bottom: `TD`, `TB`.
    Down,
    /// Bottom to top: `BT`.
    Up,
    /// Left to right: `LR`.
    Right,
    /// Right to left: `RL`.
    Left,
}

impl Direction {
    /// Whether the flow runs across the page rather than down it.
    pub fn across(self) -> bool {
        matches!(self, Direction::Right | Direction::Left)
    }
}

/// One box of the chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    /// What the box says: its label, or its id when it has none.
    pub label: String,
    pub shape: Shape,
}

/// What a box is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `A[...]`, and a node named with no label.
    Box,
    /// `A(...)`, `A([...])`, `A((...))`: a box with round corners.
    Round,
    /// `A{...}`: a question, whose links are its answers.
    Decision,
}

/// A link from one node to another, by their places in the chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub stroke: Stroke,
}

/// How a link is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    /// `-->`
    Arrow,
    /// `---`: no arrowhead.
    Open,
    /// `-.->`
    Dotted,
    /// `==>`
    Thick,
}

impl Stroke {
    /// Whether the link points at where it goes.
    pub fn points(self) -> bool {
        !matches!(self, Stroke::Open)
    }
}

/// The flowchart `source` writes, or `None` when it is anything else.
pub fn flowchart(source: &str) -> Option<Flowchart> {
    let mut statements = source
        .lines()
        .map(|line| line.split("%%").next().unwrap_or_default())
        .flat_map(statements)
        .map(str::trim)
        .filter(|statement| !statement.is_empty());
    let direction = direction(statements.next()?)?;
    let mut chart = Flowchart {
        direction,
        nodes: Vec::new(),
        links: Vec::new(),
    };
    for statement in statements {
        let word = statement.split_whitespace().next().unwrap_or_default();
        match word {
            "style" | "classDef" | "class" | "linkStyle" | "click" => continue,
            "subgraph" | "end" | "direction" => return None,
            _ => chain(&mut chart, statement)?,
        }
    }
    (!chart.nodes.is_empty()).then_some(chart)
}

/// The statements of `line`: what its `;` separate, where a `;` is not
/// the end of an entity (`#quot;`) or inside a quoted label.
fn statements(line: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (at, ch) in line.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ';' if !quoted && !ends_an_entity(&line[start..at]) => {
                found.push(&line[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    found.push(&line[start..]);
    found
}

/// Whether `before` ends in the `#name` of an entity a `;` closes.
fn ends_an_entity(before: &str) -> bool {
    before.rfind('#').is_some_and(|hash| {
        let name = &before[hash + 1..];
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

/// The way the header `graph TD` or `flowchart LR` says the chart reads.
fn direction(header: &str) -> Option<Direction> {
    let mut words = header.split_whitespace();
    if !matches!(words.next()?, "graph" | "flowchart") {
        return None;
    }
    match words.next() {
        None | Some("TD" | "TB") => Some(Direction::Down),
        Some("BT") => Some(Direction::Up),
        Some("LR") => Some(Direction::Right),
        Some("RL") => Some(Direction::Left),
        Some(_) => None,
    }
}

/// One statement: groups of nodes joined by links, `A & B --> C --> D`.
fn chain(chart: &mut Flowchart, statement: &str) -> Option<()> {
    let mut rest = statement;
    let mut before = group(chart, &mut rest)?;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Some(());
        }
        let (stroke, label) = link(&mut rest)?;
        let after = group(chart, &mut rest)?;
        for from in &before {
            for to in &after {
                chart.links.push(Link {
                    from: *from,
                    to: *to,
                    label: label.clone(),
                    stroke,
                });
            }
        }
        before = after;
    }
}

/// Nodes joined by `&`, each by its place in the chart.
fn group(chart: &mut Flowchart, rest: &mut &str) -> Option<Vec<usize>> {
    let mut nodes = vec![node(chart, rest)?];
    loop {
        let trimmed = rest.trim_start();
        let Some(after) = trimmed.strip_prefix('&') else {
            return Some(nodes);
        };
        *rest = after.trim_start();
        nodes.push(node(chart, rest)?);
    }
}

/// One node — its id and, when it has one, its shape and label — by its
/// place in the chart; a node named again keeps the label it was given.
fn node(chart: &mut Flowchart, rest: &mut &str) -> Option<usize> {
    let text = rest.trim_start();
    // An id is letters, digits, `_` and `-` — a `-` that opens a link
    // ends it.
    let id_end = text
        .char_indices()
        .find(|(at, c)| {
            let opens_a_link = text[*at..].starts_with("--") || text[*at..].starts_with("-.");
            !(c.is_alphanumeric() || *c == '_' || *c == '-' && !opens_a_link)
        })
        .map_or(text.len(), |(at, _)| at);
    let id = &text[..id_end];
    if id.is_empty() {
        return None;
    }
    let mut after = &text[id_end..];
    let mut labelled = None;
    for (open, close, shape) in SHAPES {
        if let Some(inner) = after.strip_prefix(open) {
            let (label, rest) = labelled_until(inner, close)?;
            labelled = Some((label, *shape));
            after = rest;
            break;
        }
    }
    if let Some(class) = after.strip_prefix(":::") {
        let end = class
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-')))
            .unwrap_or(class.len());
        after = &class[end..];
    }
    *rest = after;
    let at = match chart.nodes.iter().position(|node| node.id == id) {
        Some(at) => at,
        None => {
            chart.nodes.push(Node {
                id: id.to_string(),
                label: id.to_string(),
                shape: Shape::Box,
            });
            chart.nodes.len() - 1
        }
    };
    if let Some((label, shape)) = labelled {
        chart.nodes[at].label = label;
        chart.nodes[at].shape = shape;
    }
    Some(at)
}

/// The brackets a node's label sits in, longest first, and the shape each
/// draws.
const SHAPES: &[(&str, &str, Shape)] = &[
    ("([", "])", Shape::Round),
    ("((", "))", Shape::Round),
    ("[[", "]]", Shape::Box),
    ("[(", ")]", Shape::Box),
    ("{{", "}}", Shape::Decision),
    ("[", "]", Shape::Box),
    ("(", ")", Shape::Round),
    ("{", "}", Shape::Decision),
    (">", "]", Shape::Box),
];

/// A label up to `close` — quoted, or as written — with its entities
/// read, and what follows `close`.
fn labelled_until<'a>(text: &'a str, close: &str) -> Option<(String, &'a str)> {
    let text = text.trim_start();
    if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted.find('"')?;
        let after = quoted[end + 1..].trim_start().strip_prefix(close)?;
        return Some((said(&quoted[..end]), after));
    }
    let end = text.find(close)?;
    Some((said(&text[..end]), &text[end + close.len()..]))
}

/// A link, and its label when it has one: `-->`, `---`, `-.->`, `==>`,
/// labelled `-->|label|` or `-- label -->`.
fn link(rest: &mut &str) -> Option<(Stroke, Option<String>)> {
    let text = rest.trim_start();
    // A label written inside the link: `-- yes -->`, `-. maybe .->`,
    // `== sure ==>`.
    for (open, closes) in INSIDE {
        if let Some(inner) = text.strip_prefix(open) {
            if inner.starts_with(' ') {
                for (close, stroke) in *closes {
                    if let Some(end) = inner.find(close) {
                        *rest = &inner[end + close.len()..];
                        return Some((*stroke, Some(said(inner[..end].trim()))));
                    }
                }
            }
        }
    }
    // The link as one token: its line, however long, and its arrowhead.
    let body = text
        .find(|c: char| !matches!(c, '-' | '=' | '.'))
        .unwrap_or(text.len());
    let points = text[body..].starts_with('>');
    if body < 2 || body < 3 && !points {
        return None;
    }
    let line = &text[..body];
    let stroke = if line.contains('.') {
        Stroke::Dotted
    } else if line.contains('=') {
        Stroke::Thick
    } else if points {
        Stroke::Arrow
    } else {
        Stroke::Open
    };
    let mut after = &text[body + usize::from(points)..];
    let label = match after.trim_start().strip_prefix('|') {
        Some(labelled) => {
            let end = labelled.find('|')?;
            after = &labelled[end + 1..];
            Some(said(labelled[..end].trim()))
        }
        None => None,
    };
    *rest = after;
    Some((stroke, label))
}

/// The links a label is written inside, by how each opens and closes.
const INSIDE: &[(&str, &[(&str, Stroke)])] = &[
    ("--", &[("-->", Stroke::Arrow), ("---", Stroke::Open)]),
    ("-.", &[(".->", Stroke::Dotted), (".-", Stroke::Dotted)]),
    ("==", &[("==>", Stroke::Thick), ("===", Stroke::Thick)]),
];

/// A label as a reader reads it: its entities read, a line break as a
/// space, and its spaces collapsed.
fn said(label: &str) -> String {
    let mut text = label
        .replace("<br/>", " ")
        .replace("<br>", " ")
        .replace("<br />", " ");
    for (entity, ch) in [
        ("#quot;", "\""),
        ("#amp;", "&"),
        ("#lt;", "<"),
        ("#gt;", ">"),
        ("#nbsp;", " "),
        ("#35;", "#"),
        ("#59;", ";"),
    ] {
        text = text.replace(entity, ch);
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "graph TD\n subgraph one\n A --> B\n end",
            "graph TD\n A -> B",
            "graph XY\n A --> B",
            "graph TD",
        ] {
            assert_eq!(flowchart(source), None, "{source}");
        }
    }
}
