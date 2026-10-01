//! Where a value is in the text it was read from.
//!
//! The parser every document goes through says where it stopped when
//! it could not read one, and nothing about a value it read: a workflow
//! that parses and then breaks a rule — two nodes of one id, a
//! `depends_on` naming nothing — has no position to show. This is a
//! second read of the same text, by a parser that keeps the position of
//! every key and value, used only to say where a problem is.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_saphyr::granit_parser::{Event, Parser, Span};

/// A place in a text: the line and column it starts at, both counted
/// from one as an editor counts them, and how many characters it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Location {
    pub line: usize,
    pub col: usize,
    pub len: usize,
}

/// One step from a value to one inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The value under this key of a mapping.
    Key(String),
    /// The item at this position of a sequence, from zero.
    Index(usize),
    /// The node of a workflow with this id, wherever it is declared
    /// under the value it is taken from — a top-level node or a child
    /// of a `parallel` group. A fan-out sibling is found where its base
    /// node is declared.
    Node(String),
}

/// Where a value is inside a document, from its root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pointer(Vec<Step>);

impl Pointer {
    /// The document's root.
    pub fn root() -> Self {
        Pointer::default()
    }

    /// This pointer, one key further in.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.0.push(Step::Key(key.into()));
        self
    }

    /// This pointer, one sequence item further in.
    pub fn index(mut self, index: usize) -> Self {
        self.0.push(Step::Index(index));
        self
    }

    /// This pointer, at the workflow node `id` under it.
    pub fn node(mut self, id: impl Into<String>) -> Self {
        self.0.push(Step::Node(id.into()));
        self
    }

    /// This pointer followed by `rest`.
    pub fn join(mut self, rest: Pointer) -> Self {
        self.0.extend(rest.0);
        self
    }

    /// The pointer a deserializer's path names: `nodes[2].artifacts`,
    /// with `.` or nothing for the root.
    pub fn parse_path(path: &str) -> Self {
        let mut pointer = Pointer::root();
        for part in path.split('.').filter(|part| !part.is_empty()) {
            let (key, indexes) = part
                .split_once('[')
                .map_or((part, ""), |(key, rest)| (key, rest));
            if !key.is_empty() {
                pointer = pointer.key(key);
            }
            for index in indexes.split('[') {
                if let Ok(index) = index.trim_end_matches(']').parse() {
                    pointer = pointer.index(index);
                }
            }
        }
        pointer
    }

    pub fn steps(&self) -> &[Step] {
        &self.0
    }
}

/// A value read with its place.
#[derive(Debug, Clone)]
struct Placed {
    at: Location,
    value: Tree,
}

#[derive(Debug, Clone)]
enum Tree {
    Scalar(String),
    Sequence(Vec<Placed>),
    Mapping(Vec<Entry>),
}

#[derive(Debug, Clone)]
struct Entry {
    key: String,
    key_at: Location,
    value: Placed,
}

/// Every value of a text, with where it is. A text that does not read as
/// YAML has no values, and locates nothing.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    root: Option<Placed>,
}

impl SourceMap {
    /// The values of `text`, each with its place.
    pub fn read(text: &str) -> Self {
        Builder::default().read(text).unwrap_or_default()
    }

    /// Where the value `pointer` names starts, when the text has it.
    pub fn locate(&self, pointer: &Pointer) -> Option<Location> {
        self.find(pointer).map(|placed| placed.at)
    }

    /// Where the key `pointer` ends on is written, or — for a pointer
    /// that does not end on a key — where its value is.
    pub fn locate_key(&self, pointer: &Pointer) -> Option<Location> {
        match pointer.steps().split_last() {
            Some((Step::Key(key), above)) => match &self.find(&Pointer(above.to_vec()))?.value {
                Tree::Mapping(entries) => entries
                    .iter()
                    .find(|entry| &entry.key == key)
                    .map(|entry| entry.key_at),
                Tree::Scalar(_) | Tree::Sequence(_) => None,
            },
            _ => self.locate(pointer),
        }
    }

    /// Where a reader looks for what `pointer` names: the value when it is
    /// one scalar — `implementr` in `runner: implementr` — and the key
    /// otherwise; and when the text does not write that value, the
    /// nearest place above it that it does write, short of the document
    /// as a whole.
    pub fn place(&self, pointer: &Pointer) -> Option<Location> {
        let mut steps = pointer.steps().to_vec();
        while !steps.is_empty() {
            let here = Pointer(steps.clone());
            if let Some(found) = self.find(&here) {
                return match (&found.value, steps.last()) {
                    (Tree::Scalar(_), _) => Some(found.at),
                    (_, Some(Step::Key(_))) => self.locate_key(&here),
                    // An entry of a list — a node, a task — is where its
                    // first key is written.
                    (Tree::Mapping(entries), _) => entries
                        .first()
                        .map_or(Some(found.at), |entry| Some(entry.key_at)),
                    (Tree::Sequence(_), _) => Some(found.at),
                };
            }
            steps.pop();
        }
        None
    }

    /// The keys of the mapping `pointer` names, each with where it is
    /// written, in the order the text has them.
    pub fn keys_at(&self, pointer: &Pointer) -> Vec<(String, Location)> {
        match self.find(pointer).map(|placed| &placed.value) {
            Some(Tree::Mapping(entries)) => entries
                .iter()
                .map(|entry| (entry.key.clone(), entry.key_at))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn find(&self, pointer: &Pointer) -> Option<&Placed> {
        pointer
            .steps()
            .iter()
            .try_fold(self.root.as_ref()?, |placed, step| step_into(placed, step))
    }
}

fn step_into<'a>(placed: &'a Placed, step: &Step) -> Option<&'a Placed> {
    match (&placed.value, step) {
        (Tree::Mapping(entries), Step::Key(key)) => entries
            .iter()
            .find(|entry| &entry.key == key)
            .map(|entry| &entry.value),
        (Tree::Sequence(items), Step::Index(index)) => items.get(*index),
        (_, Step::Node(id)) => {
            let base = id.split_once('@').map_or(id.as_str(), |(base, _)| base);
            declaring(placed, base)
        }
        _ => None,
    }
}

/// The mapping under `placed` whose `id` is `id`, searched depth first.
fn declaring<'a>(placed: &'a Placed, id: &str) -> Option<&'a Placed> {
    match &placed.value {
        Tree::Mapping(entries) => {
            let named = entries.iter().any(|entry| {
                entry.key == "id" && matches!(&entry.value.value, Tree::Scalar(s) if s == id)
            });
            match named {
                true => Some(placed),
                false => entries.iter().find_map(|entry| declaring(&entry.value, id)),
            }
        }
        Tree::Sequence(items) => items.iter().find_map(|item| declaring(item, id)),
        Tree::Scalar(_) => None,
    }
}

/// A collection being read: where it starts, the anchor it carries,
/// and what it holds so far — for a mapping, also the key its next value
/// goes under.
enum Open {
    Sequence(Location, usize, Vec<Placed>),
    Mapping(Location, usize, Vec<Entry>, Option<(String, Location)>),
}

#[derive(Default)]
struct Builder {
    open: Vec<Open>,
    anchors: HashMap<usize, Placed>,
    root: Option<Placed>,
}

impl Builder {
    fn read(mut self, text: &str) -> Option<SourceMap> {
        let mut parser = Parser::new_from_str(text);
        while let Some(next) = parser.next_event() {
            let (event, span) = next.ok()?;
            match event {
                Event::Scalar(value, _, anchor, _) => {
                    let placed = Placed {
                        at: location(span),
                        value: Tree::Scalar(value.into_owned()),
                    };
                    self.close(anchor, placed);
                }
                Event::Alias(anchor) => {
                    // Where the alias is used, holding what it stands for.
                    let mut placed = self.anchors.get(&anchor)?.clone();
                    placed.at = location(span);
                    self.close(0, placed);
                }
                Event::SequenceStart(_, anchor, _) => {
                    self.open
                        .push(Open::Sequence(location(span), anchor, Vec::new()));
                }
                Event::MappingStart(_, anchor, _) => {
                    self.open
                        .push(Open::Mapping(location(span), anchor, Vec::new(), None));
                }
                Event::SequenceEnd | Event::MappingEnd => {
                    let (at, anchor, value) = match self.open.pop()? {
                        Open::Sequence(at, anchor, items) => (at, anchor, Tree::Sequence(items)),
                        Open::Mapping(at, anchor, entries, _) => {
                            (at, anchor, Tree::Mapping(entries))
                        }
                    };
                    self.close(anchor, Placed { at, value });
                }
                Event::StreamEnd => break,
                // The stream's and the document's own markers, and the
                // comments between values, hold no value to place.
                _ => {}
            }
        }
        Some(SourceMap { root: self.root })
    }

    /// Hands a finished value to whatever holds it, and remembers it
    /// under its anchor for the aliases that use it.
    fn close(&mut self, anchor: usize, placed: Placed) {
        if anchor != 0 {
            self.anchors.insert(anchor, placed.clone());
        }
        match self.open.last_mut() {
            None => self.root = Some(placed),
            Some(Open::Sequence(_, _, items)) => items.push(placed),
            Some(Open::Mapping(_, _, entries, key)) => match key.take() {
                None => {
                    let text = match &placed.value {
                        Tree::Scalar(text) => text.clone(),
                        Tree::Sequence(_) | Tree::Mapping(_) => String::new(),
                    };
                    *key = Some((text, placed.at));
                }
                Some((key, key_at)) => entries.push(Entry {
                    key,
                    key_at,
                    value: placed,
                }),
            },
        }
    }
}

/// Where a span starts, as an editor counts it.
fn location(span: Span) -> Location {
    Location {
        line: span.start.line(),
        col: span.start.col() + 1,
        len: span.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKFLOW: &str = "name: fix\nnodes:\n  - id: lint\n    kind: bash\n    run: cargo clippy\n  - id: fix\n    kind: prompt\n    runner: implementr\n    depends_on: [lint]\n";

    fn at(line: usize, col: usize, len: usize) -> Option<Location> {
        Some(Location { line, col, len })
    }

    #[test]
    fn a_marker_becomes_a_one_based_line_and_column() {
        let map = SourceMap::read(WORKFLOW);
        assert_eq!(map.locate(&Pointer::root().key("name")), at(1, 7, 3));
        assert_eq!(map.locate_key(&Pointer::root().key("name")), at(1, 1, 4));
    }

    #[test]
    fn a_key_of_a_nested_node_is_located() {
        let map = SourceMap::read(WORKFLOW);
        let runner = Pointer::root().key("nodes").node("fix").key("runner");
        assert_eq!(map.locate_key(&runner), at(8, 5, 6));
        assert_eq!(map.locate(&runner), at(8, 13, 10));
        assert_eq!(
            map.locate(&Pointer::root().key("nodes").node("fix@codex").key("runner")),
            at(8, 13, 10),
            "a fan-out sibling is found where its base node is declared"
        );
    }

    #[test]
    fn a_deserializer_path_is_located() {
        let map = SourceMap::read(WORKFLOW);
        assert_eq!(
            Pointer::parse_path("nodes[1].depends_on[0]"),
            Pointer::root()
                .key("nodes")
                .index(1)
                .key("depends_on")
                .index(0)
        );
        assert_eq!(
            map.locate(&Pointer::parse_path("nodes[1].depends_on[0]")),
            at(9, 18, 4)
        );
        assert_eq!(Pointer::parse_path("."), Pointer::root());
    }

    #[test]
    fn a_flow_mapping_value_is_located() {
        let map = SourceMap::read("nodes:\n  - { id: build, kind: bash, run: \"exit 1\" }\n");
        let run = Pointer::root().key("nodes").node("build").key("run");
        assert_eq!(map.locate_key(&run), at(2, 30, 3));
        assert_eq!(
            map.locate(&run),
            at(2, 35, 8),
            "a quoted value starts at its quote"
        );
    }

    #[test]
    fn an_alias_is_located_where_it_is_used() {
        let map = SourceMap::read("base: &shared\n  kind: bash\nnodes:\n  - *shared\n");
        assert_eq!(
            map.locate(&Pointer::root().key("nodes").index(0)),
            at(4, 5, 7)
        );
        assert_eq!(
            map.locate(&Pointer::root().key("nodes").index(0).key("kind")),
            at(2, 9, 4),
            "inside it, a value is where the anchor declares it"
        );
    }

    #[test]
    fn the_keys_of_a_mapping_are_listed_where_they_are_written() {
        let map = SourceMap::read(WORKFLOW);
        let keys: Vec<String> = map
            .keys_at(&Pointer::root().key("nodes").node("lint"))
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        assert_eq!(keys, ["id", "kind", "run"]);
    }

    #[test]
    fn a_place_is_the_value_a_reader_fixes_or_the_nearest_place_written() {
        let map = SourceMap::read(WORKFLOW);
        let fix = Pointer::root().key("nodes").node("fix");
        assert_eq!(map.place(&fix.clone().key("runner")), at(8, 13, 10));
        assert_eq!(map.place(&fix.clone().key("depends_on")), at(9, 5, 10));
        assert_eq!(
            map.place(&fix.key("run").key("command")),
            at(6, 5, 2),
            "a key the node does not write falls back to the node"
        );
        assert_eq!(map.place(&Pointer::root().key("nowhere")), None);
    }

    #[test]
    fn a_text_that_is_not_yaml_locates_nothing() {
        let map = SourceMap::read("nodes: [unclosed\n");
        assert_eq!(map.locate(&Pointer::root().key("nodes")), None);
    }
}
