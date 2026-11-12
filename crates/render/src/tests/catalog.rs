//! The catalog as a person moves through it, in every environment: each workflow
//! led by the pointer's place, the one pointed at carrying it, and one whose file
//! does not read in line with the others.

use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Markdown, Surface, Terminal};
use yunta_core::Workflow;

use crate::page::{
    Catalog, OfferedWorkflow, Page, PointedCatalog, ReadableWorkflow, UnreadableWorkflow,
};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/catalog")
}

/// A workflow called `name`, that does what `description` says.
fn readable(name: &str, description: &str) -> OfferedWorkflow {
    let yaml = format!(
        "name: {name}\ndescription: {description}\nnodes:\n  - {{ id: only, kind: bash, run: \"true\" }}\n"
    );
    let workflow: Workflow = yunta_core::yaml::parse(&yaml).expect("a workflow");
    OfferedWorkflow {
        name: name.to_string(),
        workflow: Ok(ReadableWorkflow {
            workflow,
            usually: None,
        }),
    }
}

/// Two workflows that read and one that does not.
fn catalog() -> Catalog {
    Catalog {
        broken_packs: Vec::new(),
        workflows_dir: PathBuf::from(".yunta/workflows"),
        packs_dir: PathBuf::from(".yunta/packs"),
        offered: vec![
            readable("greet", "says hello"),
            readable("lint-fix", "a check, and an agent that fixes what it finds"),
            OfferedWorkflow {
                name: "broken".to_string(),
                workflow: Err(UnreadableWorkflow::File("no such file".to_string())),
            },
        ],
    }
}

#[test]
fn the_catalog_pointed_at_matches_its_goldens_in_every_environment() {
    let catalog = catalog();
    let page = PointedCatalog {
        catalog: &catalog,
        pointed: "lint-fix",
    };
    for environment in &ENVIRONMENTS {
        let look = Look::of(environment);
        let drawn = Terminal::on(look).draw(&page.document(&look));
        assert_golden(&environment.golden(&goldens(), "pointed"), &drawn);
    }
}

/// The catalog `yunta list` prints carries no pointer: what the application points
/// at leaves the page it shares with the CLI and the control plane as it was.
#[test]
fn the_catalog_read_whole_carries_no_pointer() {
    let catalog = catalog();
    let drawn = Markdown.draw(&catalog.document(&Markdown.look()));
    assert!(!drawn.contains('▸'), "{drawn}");
}
