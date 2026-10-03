//! A diagram an author wrote, as each surface draws it: a file keeps the
//! Mermaid source as written, and a terminal draws what it can read of
//! it.

mod across;
mod down;
pub mod flowchart;
mod grid;
mod layer;
mod outline;
mod place;
pub(crate) mod split;

pub use flowchart::{flowchart, Direction, Flowchart, Link, Node, Shape, Stroke};

use crate::blocks::{Code, Drawn};
use crate::ink::{Line, Tone};
use crate::{wrap, Look, INDENT};

/// A diagram as an author wrote it, and the flowchart it reads as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagram {
    /// The Mermaid source, as written.
    pub source: String,
    /// The flowchart it reads as; `None` for anything this does not read.
    pub chart: Option<Flowchart>,
}

impl Diagram {
    /// The diagram `source` writes.
    pub fn of(source: &str) -> Self {
        Diagram {
            source: source.to_string(),
            chart: flowchart(source),
        }
    }
}

impl Drawn for Diagram {
    /// A chart this reads in boxes, the way its author wrote it when it
    /// fits the line, turned when only that fits, and one line per chain
    /// when neither does; any other diagram as its source, saying it is
    /// not drawn here. A chart that reads up or left is drawn reading
    /// down or right: the same boxes and links, the other way up.
    fn lines(&self, look: &Look) -> Vec<Line> {
        let Some(chart) = &self.chart else {
            return Code::whole(
                "diagram",
                Some(
                    "a diagram this terminal does not draw; it is drawn where Markdown is read"
                        .to_string(),
                ),
                &self.source,
            )
            .lines(look);
        };
        let room = look.width.cells().saturating_sub(INDENT.len());
        let across = chart.direction.across();
        let boxed = place::drawn(chart, across, room, look.glyphs)
            .or_else(|| place::drawn(chart, !across, room, look.glyphs));
        match boxed {
            Some(rows) => rows
                .into_iter()
                .map(|row| Line::new().plain(INDENT).plain(row))
                .collect(),
            None => chains(chart, look),
        }
    }
}

/// `chart` as one line per chain, each wrapped under itself.
fn chains(chart: &Flowchart, look: &Look) -> Vec<Line> {
    let under = format!("{INDENT}  ");
    let room = look.width.cells().saturating_sub(under.len());
    outline::outline(chart, look.glyphs)
        .iter()
        .flat_map(|chain| {
            wrap(chain, room)
                .into_iter()
                .enumerate()
                .map(|(at, part)| match at {
                    0 => Line::new().plain(INDENT).push(Tone::Strong, part),
                    _ => Line::new().plain(under.as_str()).push(Tone::Strong, part),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::split::{parts, Part};
    use super::*;
    use crate::doc::{Block, Doc};
    use crate::surface::{Markdown, Surface, Terminal};
    use crate::Glyphs;

    /// Markdown with a paragraph, a flowchart in a list item, and a
    /// paragraph after it.
    const AUTHORED: &str = "The run plans, then asks.\n\n- the flow:\n  ```mermaid\n  graph LR\n    plan -->|tasks| gate{Approve?}\n    gate -->|yes| build\n    gate -- no --> plan\n  ```\n\nThen it builds.";

    fn plain(width: usize, glyphs: Glyphs) -> Look {
        Look {
            glyphs,
            width: crate::Width::of(Some(100), None).within(width),
            ..Look::plain()
        }
    }

    #[test]
    fn authored_markdown_reads_as_its_text_and_its_diagrams_in_order() {
        let read = parts(AUTHORED);
        assert_eq!(read.len(), 3, "{read:?}");
        assert_eq!(
            read[0],
            Part::Text("The run plans, then asks.\n\n- the flow:".to_string())
        );
        let Part::Diagram(diagram) = &read[1] else {
            panic!("{read:?}");
        };
        assert!(
            diagram.source.starts_with("graph LR\n  plan -->|tasks|"),
            "{}",
            diagram.source
        );
        assert!(diagram.chart.is_some());
        assert_eq!(read[2], Part::Text("Then it builds.".to_string()));
    }

    #[test]
    fn a_chart_reads_as_one_line_per_chain_in_the_order_it_flows() {
        let chart = flowchart("graph LR\n plan -->|tasks| gate{Approve?}\n gate -->|yes| build\n gate -- no --> plan\n lone").unwrap();
        assert_eq!(
            outline::outline(&chart, Glyphs::Unicode),
            [
                "plan → (tasks) Approve?",
                "Approve? → (yes) build",
                "Approve? → (no) plan",
                "lone",
            ]
        );
        let open = flowchart("graph TD\n a --- b --> c").unwrap();
        assert_eq!(outline::outline(&open, Glyphs::Ascii), ["a -- b -> c"]);
    }

    #[test]
    fn a_diagram_a_terminal_does_not_draw_is_shown_as_written() {
        let diagram = Diagram::of("sequenceDiagram\n  A->>B: hi");
        let look = plain(80, Glyphs::Unicode);
        let drawn: Vec<String> = diagram
            .lines(&look)
            .iter()
            .map(|line| look.ink.paint(line))
            .collect();
        assert!(
            drawn
                .iter()
                .any(|line| line.contains("a diagram this terminal does not draw"))
                && drawn.iter().any(|line| line.contains("A->>B: hi")),
            "{drawn:?}"
        );
    }

    #[test]
    fn a_file_keeps_the_diagram_as_written_and_a_terminal_draws_it() {
        let doc = Doc::new().with(Block::Markdown(AUTHORED.to_string()));
        assert_eq!(Markdown.draw(&doc), format!("{AUTHORED}\n"));

        let drawn = Terminal::on(plain(80, Glyphs::Unicode)).draw(&doc);
        assert!(
            drawn.contains("│ plan │── tasks ▶│ Approve? │") && !drawn.contains("```"),
            "{drawn}"
        );
        assert!(drawn.contains("Then it builds."), "{drawn}");
    }

    /// A chart written to read across, with a question and a link back.
    const LR: &str = "graph LR\n  plan[Write the plan] --> spec[Write its tests] --> gate{Approve?}\n  gate -->|yes| build[Build each task]\n  gate -->|no| plan";

    /// A chart written to read down, its question answered both ways.
    const DECIDED: &str = "graph TD\n  idea[Read the idea] --> clear{Is it clear?}\n  clear -->|yes| plan(Plan it)\n  clear -.->|no| ask[Ask a question]\n  ask ==> idea";

    /// A chart written to read down whose one layer is too wide for a
    /// narrow line, and fits it turned.
    const TURNED: &str = "graph TD\n  run[The run] --> grill[Ask what only a person decides] & brief[Write what was asked for] & plan[Plan the change task by task]";

    /// A chart whose loop back carries a label longer than the box it
    /// reaches is wide.
    const LOOPED: &str = "graph LR\n  add[pack add --global] --> user[the user's packs]\n  user --> find{a project looks for a pack}\n  find -->|its own| project[the project's pack]\n  find -->|none of its own| user";

    /// A chart too long to read across a line and too wide to read down
    /// it: a pipeline that ends fanning out.
    const CHAINED: &str = "graph LR\n  a[Read the idea and its brief] --> b[Plan the change task by task] --> c[Write the tests for the plan] --> d[Approve the plan and its tests]\n  d --> e[Build the store task] & f[Build the command task] & g[Build the docs task] & h[Build the release task]";

    #[test]
    fn a_chart_is_drawn_in_boxes_as_it_fits_and_matches_its_goldens() {
        let goldens = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/diagram");
        for (name, source) in [
            ("lr", LR),
            ("labels-and-decision", DECIDED),
            ("turned", TURNED),
            ("looped", LOOPED),
            ("outline", CHAINED),
        ] {
            let doc = Doc::new().with(Block::Diagram(Diagram::of(source)));
            for environment in &yunta_testkit_core::golden::ENVIRONMENTS {
                let look = Look::of(environment);
                let drawn = Terminal::on(look).draw(&doc);
                yunta_testkit_core::golden::assert_golden(
                    &environment.golden(&goldens, name),
                    &drawn,
                );
                let plain = crate::ink::strip_sgr(&drawn);
                assert!(
                    plain
                        .lines()
                        .all(|line| crate::cell_width(line) <= look.width.cells()),
                    "{name} runs past the line:\n{plain}"
                );
                if look.glyphs == Glyphs::Ascii {
                    assert!(plain.is_ascii(), "{name} draws outside ASCII:\n{plain}");
                }
            }
        }
    }
}
