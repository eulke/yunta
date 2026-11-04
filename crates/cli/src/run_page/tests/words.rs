//! The words a page says of where a run works and of where else to read it.

use std::path::Path;

use surco::doc::Depth;
use surco::look::Look;
use yunta_core::Isolation;

use super::super::fixtures::{cases, outline, Case, RUN};
use super::super::Scope;
use super::draw;
use crate::surface::Outline;

/// The first case whose page ends on a decision, and the first whose page does not.
fn decided_and_not() -> (Case, Case) {
    let (decided, others): (Vec<Case>, Vec<Case>) = cases()
        .into_iter()
        .partition(|case| case.decision.is_some());
    (
        decided.into_iter().next().expect("a decision's fixture"),
        others
            .into_iter()
            .next()
            .expect("a fixture with no decision"),
    )
}

/// The row a run that worked in this checkout says it did, read from `cwd`.
fn branch_read_from(cwd: &'static Path) -> String {
    let case = decided_and_not().1;
    let mut page = case.page(Scope::Closing, Depth::Glance);
    page.paths = Outline {
        worktree: Path::new("/home/me/project"),
        isolation: Isolation::None,
        cwd,
        ..outline()
    };
    draw(&page, &Look::plain())
        .lines()
        .find(|line| line.trim_start().starts_with("branch"))
        .expect("a closing names its branch")
        .trim()
        .to_string()
}

/// `.` names nothing a person did not know: they are standing there.
#[test]
fn a_run_in_this_checkout_says_so_without_naming_the_directory_it_stands_in() {
    assert_eq!(
        branch_read_from(Path::new("/home/me/project")),
        "branch       main, in this checkout"
    );
    assert_eq!(
        branch_read_from(Path::new("/home/me/other")),
        "branch       main, in the checkout at ~/project"
    );
}

/// The page at a glance, for a run that worked in this checkout, read from `cwd`.
fn glance_read_from(cwd: &'static Path) -> String {
    let case = decided_and_not().0;
    let mut page = case.page(Scope::Status, Depth::Glance);
    page.paths = Outline {
        worktree: Path::new("/home/me/project"),
        isolation: Isolation::None,
        cwd,
        ..outline()
    };
    draw(&page, &Look::plain())
}

/// A reader elsewhere copies the tree into another terminal; one standing in it has it.
#[test]
fn a_glance_names_the_tree_only_to_a_reader_who_stands_elsewhere() {
    let here = glance_read_from(Path::new("/home/me/project"));
    assert!(!here.contains("the run works in"), "{here}");
    let elsewhere = glance_read_from(Path::new("/home/me/other"));
    assert!(
        elsewhere.contains("the run works in: /home/me/project"),
        "{elsewhere}"
    );
}

/// The same page in a browser is one command from the whole of it, and from nowhere the
/// run's own close is drawn nor from the browser's page itself.
#[test]
fn the_whole_page_offers_a_browser_and_a_glance_offers_the_whole_page() {
    let open = format!("yunta open {}", RUN.handle());
    let case = decided_and_not().1;
    let said = |scope, depth| draw(&case.page(scope, depth), &Look::plain());
    assert!(said(Scope::Status, Depth::Full).contains(&open));
    assert!(said(Scope::Status, Depth::Dig).contains(&open));
    assert!(!said(Scope::Status, Depth::Glance).contains(&open));
    assert!(!said(Scope::Closing, Depth::Glance).contains(&open));
    assert!(!said(Scope::Browser, Depth::Full).contains(&open));
}

/// A decision is what is most worth reading whole: at every depth `status` draws it, it
/// says where, beside what else moves the run.
#[test]
fn a_decision_offers_a_browser_beside_what_else_moves_the_run() {
    let open = format!("yunta open {}", RUN.handle());
    let case = decided_and_not().0;
    let said = |scope, depth| draw(&case.page(scope, depth), &Look::plain());
    for depth in [Depth::Glance, Depth::Full, Depth::Dig] {
        let drawn = said(Scope::Status, depth);
        assert!(drawn.contains(&open), "{drawn}");
        assert!(drawn.contains("the whole page, in a browser"), "{drawn}");
    }
    assert!(!said(Scope::Closing, Depth::Glance).contains(&open));
    assert!(!said(Scope::Browser, Depth::Full).contains(&open));
}
