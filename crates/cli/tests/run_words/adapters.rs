//! Every adapter a run can stand on speaks through the run's socket.

use super::*;

/// What a session says on `claude-code`, on `codex` and on the mock reaches a
/// reader in another process whole, naming its node and session, with the
/// secret the run declares taken out, while the run moves.
///
/// Reached by: `crates/adapters/src/claude_code/parse.rs`,
/// `crates/adapters/src/codex/parse.rs`, `crates/engine/src/task_cycle/stream.rs`
#[test]
fn each_adapter_speaks_through_the_runs_socket() {
    for speaker in [Speaker::ClaudeCode, Speaker::Codex, Speaker::Mock] {
        let text =
            format!("## Parser\n\nI read the lexer, token {SECRET}.\n- next: the empty input");
        let mut talking = Talking::start(speaker, &[&text]);

        let heard = yunta_testkit::said::heard_blocking(&talking.run_dir(), 1);
        talking.stop();

        let [heard] = heard.as_slice() else {
            panic!("{speaker:?}: heard {heard:?}")
        };
        assert_eq!(
            heard.said.text,
            text.replace(SECRET, "[redacted]"),
            "{speaker:?}"
        );
        assert_eq!(heard.said.node_id.as_str(), "talk", "{speaker:?}");
        assert!(!heard.said.session_id.as_str().is_empty(), "{speaker:?}");
    }
}
