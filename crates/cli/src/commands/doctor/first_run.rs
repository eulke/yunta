//! What a new binary's first run costs on this machine.
//!
//! macOS assesses every executable it has not seen before its first run,
//! and a test suite links a binary per test target: a run that rebuilds
//! them pays the assessment once per binary, every time, outside any test.
//! The probe links a binary nobody ran before, runs it twice the way a run
//! starts a command, and says what the first run cost over the second —
//! and, when that is enough to slow a suite, how to exempt the terminal
//! that runs yunta.

use std::time::{Duration, Instant};

use crate::render::blocks::Found;

/// How much more than its second run a new binary's first run may cost
/// before a suite of them slows a run: a tenth of a second, which two
/// hundred test binaries turn into twenty.
const NOTICEABLE: Duration = Duration::from_millis(100);

/// The probe's row on `doctor`'s checklist, or `None` where it does not
/// apply — another system, or no C compiler to link a probe with.
pub(super) async fn first_run() -> Option<(Found, String)> {
    if std::env::consts::OS != "macos" {
        return None;
    }
    let (first, then) = measured().await?;
    Some(verdict(first, then, NOTICEABLE))
}

/// What the first and the second run of a freshly linked binary took.
async fn measured() -> Option<(Duration, Duration)> {
    let dir = tempfile::tempdir().ok()?;
    let source = dir.path().join("probe.c");
    // A value only this probe holds, so the binary is one the system has
    // never seen, whatever ran before.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos())
        ^ u128::from(std::process::id());
    let program = format!("int main(void) {{ return {nonce} == 0; }}\n");
    tokio::fs::write(&source, program).await.ok()?;
    let binary = dir.path().join("probe");
    let linked = tokio::process::Command::new("cc")
        .arg("-o")
        .arg(&binary)
        .arg(&source)
        .output()
        .await
        .ok()?;
    if !linked.status.success() {
        return None;
    }
    let first = timed(&binary).await?;
    let then = timed(&binary).await?;
    Some((first, then))
}

/// How long one run of `binary` took.
async fn timed(binary: &std::path::Path) -> Option<Duration> {
    let started = Instant::now();
    tokio::process::Command::new(binary).status().await.ok()?;
    Some(started.elapsed())
}

/// What `doctor` says of a new binary whose first run took `first` and
/// whose second took `then`, given the cost `noticeable` over it.
pub(super) fn verdict(first: Duration, then: Duration, noticeable: Duration) -> (Found, String) {
    let cost = first.saturating_sub(then);
    let said = format!(
        "a new binary's first run takes {} here, its next {}",
        millis(first),
        millis(then)
    );
    match cost >= noticeable {
        false => (Found::Holds, said),
        true => (
            Found::Caution,
            format!(
                "{said}: macOS assesses each new executable before its first run, and a \
                 suite pays that for every test binary it links again — add the terminal that \
                 runs yunta under System Settings › Privacy & Security › Developer Tools, then \
                 open it again"
            ),
        ),
    }
}

fn millis(spent: Duration) -> String {
    format!("{} ms", spent.as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_run_that_costs_little_more_holds() {
        let (found, said) = verdict(
            Duration::from_millis(30),
            Duration::from_millis(8),
            Duration::from_millis(100),
        );
        assert_eq!(found, Found::Holds);
        assert!(said.contains("30 ms") && said.contains("8 ms"), "{said}");
    }

    #[test]
    fn a_first_run_that_costs_a_suite_minutes_says_how_to_exempt_the_terminal() {
        let (found, said) = verdict(
            Duration::from_millis(450),
            Duration::from_millis(7),
            Duration::from_millis(100),
        );
        assert_eq!(found, Found::Caution);
        assert!(said.contains("Developer Tools"), "{said}");
    }
}
