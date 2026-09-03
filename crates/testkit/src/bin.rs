//! Running the compiled `yunta` binary and reading its output.

use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Runs the compiled `yunta` binary at `bin` in `dir` with `YUNTA_HOME`
/// pointed at `home` and stdin closed, returning its captured output. Use
/// the [`yunta_in!`](crate::yunta_in) macro rather than calling this
/// directly — it fills in the binary path from the calling crate's
/// `CARGO_BIN_EXE_yunta`.
pub fn run_yunta(bin: &Path, dir: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(bin)
        .args(args)
        .current_dir(dir)
        .env("YUNTA_HOME", home)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run the yunta binary")
}

/// The command's stdout as an owned `String` (lossy on non-UTF-8).
pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The command's stderr as an owned `String` (lossy on non-UTF-8).
pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The run id `yunta run` prints, parsed from a `run <id>: …` line — the
/// handle every follow-up command (`status`, `receipt`, `graph --run`)
/// needs. Panics if no such line is present, naming what it saw.
pub fn run_id_from(output: &Output) -> String {
    let text = stdout(output);
    text.lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .unwrap_or_else(|| panic!("no `run <id>:` line in output:\n{text}"))
}
