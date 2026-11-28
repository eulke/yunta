//! The first request's words: each workflow the catalog offers with what it does, in the
//! catalog's order, its name in a column of sixteen cells, those that do not read left to the
//! printed catalog; and its legend, which names no key a person types with, since every
//! printable key types there, and says where `esc` goes.

use std::path::PathBuf;

use surco::components::Keys;
use surco::doc::Block;
use surco::line::Line;
use surco::look::Look;
use yunta_core::Workflow;

use super::FirstRequest;
use crate::page::{Catalog, OfferedWorkflow, Page, ReadableWorkflow, UnreadableWorkflow};

/// The workflow a person runs as `name`, a pack's own under its pack's name, saying
/// `description` where it has one.
fn offered(name: &str, description: Option<&str>) -> OfferedWorkflow {
    let described = description
        .map(|description| format!("description: {description}\n"))
        .unwrap_or_default();
    let own = name.rsplit('/').next().unwrap_or(name);
    let yaml =
        format!("name: {own}\n{described}nodes:\n  - {{ id: only, kind: bash, run: \"true\" }}\n");
    let workflow: Workflow =
        yunta_core::workflow::read::read(&yaml, std::path::Path::new("w.yaml"))
            .expect("a workflow");
    OfferedWorkflow {
        name: name.to_string(),
        workflow: Ok(ReadableWorkflow {
            workflow,
            past: None,
        }),
    }
}

/// A catalog offering a workflow that says what it does, one that does not read, and one
/// that says nothing of itself, in that order.
fn catalog() -> Catalog {
    let broken = OfferedWorkflow {
        name: "broken".to_string(),
        workflow: Err(UnreadableWorkflow::File("no such file".to_string())),
    };
    Catalog {
        broken_packs: Vec::new(),
        workflows_dir: PathBuf::from(".yunta/workflows"),
        packs_dir: PathBuf::from(".yunta/packs"),
        offered: vec![
            offered("yunta/fragua", Some("the full cycle")),
            broken,
            offered("greet", None),
        ],
        pinnable: Vec::new(),
    }
}

/// The words of each row a list of rows holds.
fn rows(rows: &[Line]) -> Vec<String> {
    rows.iter().map(Line::text).collect()
}

#[test]
fn it_lists_each_workflow_with_what_it_does_in_the_catalogs_order() {
    let catalog = catalog();
    let request = FirstRequest {
        before: "",
        after: "",
        catalog: Some(&catalog),
    };
    let blocks = request.document(&Look::plain()).into_blocks();
    let listed = blocks.iter().find_map(|block| match block {
        Block::Rows(listed) => Some(rows(listed)),
        _ => None,
    });
    assert_eq!(
        listed,
        Some(vec![
            "yunta/fragua    the full cycle".to_string(),
            "greet".to_string(),
        ]),
        "each name in a column of sixteen cells, what it does after it"
    );
    let asked = blocks.iter().find_map(|block| match block {
        Block::TypedAnswer(asked) => Some(asked),
        _ => None,
    });
    let asked = asked.expect("the question over the box its words are typed in");
    assert_eq!(asked.asked.text(), "What do you want to build?");
    assert_eq!(
        asked.placeholder.as_deref(),
        Some("add support for installing a pack globally")
    );
}

/// Before the catalog is read, the question and its box stand alone.
#[test]
fn before_the_catalog_is_read_the_question_stands_alone() {
    let request = FirstRequest {
        before: "",
        after: "",
        catalog: None,
    };
    let blocks = request.document(&Look::plain()).into_blocks();
    assert!(
        matches!(blocks.as_slice(), [Block::TypedAnswer(_)]),
        "{} blocks",
        blocks.len()
    );
}

/// The first request's keys: what typing does, going on to a new run, and `esc` going to the
/// field, whatever the field holds; no key that types names an act.
#[test]
fn its_legend_names_no_printable_key() {
    let legend = crate::screen::first_request_legend();
    assert_eq!(
        crate::tests::screen::spelled(&legend),
        "type what you want to build · enter new run · esc field",
        "`esc` goes to the field, whatever it holds"
    );
    let printable = legend.0.iter().find(|binding| match &binding.keys {
        Keys::Typed(keys) => keys.chars().count() == 1,
        Keys::UpDown | Keys::LeftRight | Keys::Typing => false,
    });
    assert!(
        printable.is_none(),
        "a key that types names an act: {}",
        crate::tests::screen::spelled(&legend)
    );
}

/// What a workflow says it does, written over lines in its file as a YAML block writes it,
/// stands on its one row as one line: the first request's and the workflows' alike.
#[test]
fn what_a_workflow_does_written_over_lines_stands_on_one_row() {
    let mut catalog = catalog();
    catalog.offered = vec![offered("yunta/fragua", Some("|\n  the full\n  cycle"))];
    let request = FirstRequest {
        before: "",
        after: "",
        catalog: Some(&catalog),
    };
    let blocks = request.document(&Look::plain()).into_blocks();
    let listed = blocks.iter().find_map(|block| match block {
        Block::Rows(listed) => Some(rows(listed)),
        _ => None,
    });
    assert_eq!(
        listed,
        Some(vec!["yunta/fragua    the full cycle".to_string()])
    );
    let pointed = crate::page::PointedCatalog {
        catalog: &catalog,
        pointed: "yunta/fragua",
    };
    let on_screen = pointed.on_screen(&Look::plain()).into_blocks();
    let listed = on_screen.iter().find_map(|block| match block {
        Block::Rows(listed) => Some(rows(listed)),
        _ => None,
    });
    let listed = listed.expect("the workflows a row each");
    assert!(
        listed
            .iter()
            .all(|row| !row.contains('\n') && row.ends_with("the full cycle")),
        "{listed:?}"
    );
}
