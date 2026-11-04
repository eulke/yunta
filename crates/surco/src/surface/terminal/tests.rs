use crate::components::{Chosen, Decision, DecisionOption, Fold, Next, Prose, Section, Title};
use crate::doc::{Block, Doc};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::surface::Terminal;

/// What `doc` draws plain at `cells`, line by line.
fn drawn(doc: &Doc, cells: usize) -> Vec<String> {
    let look = Look {
        width: Look::plain().width.within(cells),
        ..Look::plain()
    };
    Terminal::on(look)
        .lines(doc)
        .iter()
        .map(|line| line.text().trim_end().to_string())
        .collect()
}

/// A title too long for its line breaks between two facts, never inside
/// one, and goes on under the first fact.
#[test]
fn a_title_breaks_between_its_facts_and_goes_on_under_the_first() {
    let title = Title {
        name: Line::new().push(Tone::Strong, "plan"),
        facts: vec![
            "4 tasks in 3 steps".to_string(),
            "2 risks".to_string(),
            "2 left out".to_string(),
        ],
    };
    assert_eq!(
        drawn(&Doc::new().with(title), 30),
        ["plan: 4 tasks in 3 steps,", "      2 risks, 2 left out"]
    );
}

/// A terminal opens every fold: it draws one as the section it holds.
#[test]
fn a_fold_is_drawn_as_its_section() {
    let section = || Section {
        lead: None,
        title: Line::new().plain("card"),
        blocks: vec![Block::from(Prose("inside".to_string()))],
    };
    assert_eq!(
        drawn(&Doc::new().with(Fold(section())), 60),
        drawn(&Doc::new().with(section()), 60)
    );
}

/// A decision answered on a forge ends with what is typed elsewhere: its
/// title, then each step under it.
#[test]
fn a_decision_on_a_forge_ends_with_what_is_typed_elsewhere() {
    let decision = Decision {
        chosen: Chosen::OnTheForge {
            forge: "GitHub".to_string(),
            elsewhere: "on the machine".to_string(),
        },
        options: vec![DecisionOption {
            id: "approve".to_string(),
            label: Some("approve it".to_string()),
            tradeoff: Some("it passes".to_string()),
            asks: None,
        }],
        otherwise: vec![("yunta resume x".to_string(), "reads it")],
    };
    let lines: Vec<String> = drawn(&Doc::new().with(decision), 60)
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let [.., elsewhere, step] = lines.as_slice() else {
        panic!("a forge's decision draws its options and what follows: {lines:?}");
    };
    // Every line of a decision opens with its edge.
    assert_eq!(elsewhere.trim(), "| on the machine", "{lines:?}");
    assert!(
        step.trim().starts_with("|   yunta resume x") && step.trim().ends_with("reads it"),
        "{lines:?}"
    );
}

/// Inside a section, a fold leaves the blank line a section leaves before the
/// block that follows it.
#[test]
fn a_fold_in_a_section_is_spaced_from_what_follows_it_as_a_section_is() {
    let outer = Section {
        lead: None,
        title: Line::new().plain("outer"),
        blocks: vec![
            Fold(Section {
                lead: None,
                title: Line::new().plain("card"),
                blocks: vec![Block::from(Prose("inside".to_string()))],
            })
            .into(),
            Prose("after".to_string()).into(),
        ],
    };
    assert_eq!(
        drawn(&Doc::new().with(outer), 60),
        ["  outer", "", "    card", "      inside", "", "    after"]
    );
}

/// A gloss too long to stand beside its command at the width goes on the line
/// under it, one step in, so no line passes the width; where it fits, it stands
/// beside the command.
#[test]
fn a_gloss_too_long_for_the_line_goes_under_its_command() {
    let next = || Next {
        steps: vec![(
            "yunta resume 7E5PH4".to_string(),
            "offers a retry once the run's tree changed",
        )],
    };
    assert_eq!(
        drawn(&Doc::new().with(next()), 60),
        [
            "next",
            "  yunta resume 7E5PH4",
            "    offers a retry once the run's tree changed",
        ]
    );
    assert_eq!(
        drawn(&Doc::new().with(next()), 80),
        [
            "next",
            "  yunta resume 7E5PH4   offers a retry once the run's tree changed",
        ]
    );
}

/// A gloss stands at the column its widest command sets, counted in cells: a
/// command of wide characters pads to that column by cells too.
#[test]
fn a_gloss_stands_at_its_column_counted_in_cells() {
    let next = Next {
        steps: vec![
            ("cat 数据".to_string(), "shows it"),
            ("ls".to_string(), "lists it"),
        ],
    };
    assert_eq!(
        drawn(&Doc::new().with(next), 80),
        ["next", "  cat 数据   shows it", "  ls         lists it"]
    );
}
