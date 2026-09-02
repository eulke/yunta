use std::process::Command;

#[test]
fn the_binary_reports_its_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_yunta"))
        .arg("--version")
        .output()
        .expect("failed to run the yunta binary");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("yunta "));
}

#[test]
fn the_binary_without_a_subcommand_prints_its_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_yunta"))
        .output()
        .expect("failed to run the yunta binary");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage: yunta"));
}
