use std::process::Command;

#[test]
fn binary_runs_and_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_yunta"))
        .output()
        .expect("failed to run the yunta binary");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("yunta-engine "));
}
