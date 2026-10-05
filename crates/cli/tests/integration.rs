//! The binary answering about itself: its version, and what it says
//! when it is given nothing to do.

use yunta_testkit::{stderr, stdout, yunta_at, Checkout};

#[test]
fn the_binary_reports_its_version() {
    let output = yunta_at!(Checkout::new(), &["--version"]);

    assert!(output.status.success());
    assert!(stdout(&output).starts_with("yunta "));
}

#[test]
fn the_binary_without_a_subcommand_prints_its_usage() {
    let output = yunta_at!(Checkout::new(), &[]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("Usage: yunta"));
}
