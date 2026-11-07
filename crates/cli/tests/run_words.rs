//! What a session says reaches whoever watches its run from any process:
//! every adapter speaks through the run's socket, and the words go only to a
//! person at a terminal.

use std::path::PathBuf;

use yunta_testkit::{
    git, hermetic, init_repo, runs_root, stderr, stdout, wait_for, write, yunta_in, CliChild,
};

#[path = "run_words/adapters.rs"]
mod adapters;

#[path = "run_words/row.rs"]
mod row;

#[path = "run_words/off_terminal.rs"]
mod off_terminal;

/// A value the run declares secret, which every session here says back.
const SECRET: &str = "words-secret-value-77";

/// The CLI a session stands on.
#[derive(Clone, Copy, Debug)]
enum Speaker {
    ClaudeCode,
    Codex,
    Mock,
}

impl Speaker {
    /// The adapter the runner names, and the stub that stands in for its CLI.
    fn adapter(self) -> (&'static str, Option<PathBuf>) {
        match self {
            Speaker::ClaudeCode => (
                "claude-code",
                Some(yunta_testkit_core::stubs::claude_code()),
            ),
            Speaker::Codex => ("codex", Some(yunta_testkit_core::stubs::codex())),
            Speaker::Mock => ("mock", None),
        }
    }
}

/// A run whose one node's session, on a `Speaker`, says something and then
/// holds until the run is cancelled.
struct Talking {
    _root: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
    /// The run's process, until it is stopped.
    child: Option<CliChild>,
}

impl Talking {
    /// Starts the run off a terminal, from a project of its own.
    fn start(speaker: Speaker, said: &[&str]) -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        let home = root.path().join("state");
        project(&repo, speaker, said);
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
        hermetic(&mut command, &repo, &home);
        command.args(["run", "wf.yaml"]);
        if let Speaker::Mock = speaker {
            command.args(["--adapter", "mock", "--fixture", "fixture.yaml"]);
        }
        command
            .env("WORDS_TOKEN", SECRET)
            .env("CLAUDE_STUB_LINGER", "1")
            .env("CODEX_STUB_LINGER", "1");
        let child = CliChild::spawn(command, None).unwrap();
        Talking {
            _root: root,
            repo,
            home,
            child: Some(child),
        }
    }

    /// The run's directory, once the run has one.
    fn run_dir(&self) -> PathBuf {
        let runs = runs_root(&self.home);
        wait_for(
            || {
                std::fs::read_dir(&runs)
                    .ok()?
                    .flatten()
                    .next()
                    .map(|run| run.path())
            },
            || "the run never got a directory".to_string(),
        )
    }

    /// The run's id, once the run has a directory.
    fn id(&self) -> String {
        let run_dir = self.run_dir();
        run_dir.file_name().unwrap().to_string_lossy().into_owned()
    }

    /// Cancels the run, waits for its process to end, and answers with what
    /// it printed on both of its streams. The project stays, for what a test
    /// asks of the stopped run.
    fn stop(&mut self) -> String {
        let cancel = yunta_in!(&self.repo, &self.home, &["cancel", &self.id()]);
        assert!(cancel.status.success(), "{cancel:?}");
        let child = self.child.take().expect("a run stops once");
        let output = child.wait_with_output().unwrap();
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
}

/// How long the mock waits before each thing its session says after the
/// first: long enough that a test watching a terminal sees the row before it
/// changes, while the test still waits on what is drawn.
const SAYS_AGAIN_AFTER_MS: u64 = 2000;

/// Writes `repo` as a project whose one node's session, on `speaker`, says
/// each of `said` in turn and then holds, and commits it.
fn project(repo: &std::path::Path, speaker: Speaker, said: &[&str]) {
    std::fs::create_dir_all(repo).unwrap();
    init_repo(repo);
    let (adapter, binary) = speaker.adapter();
    // A session that holds is ended by its node's timeout within a minute
    // if a test dies before it cancels the run.
    let mut config = format!(
        "defaults:\n  isolation: none\n  timeout_minutes: 1\nrunners:\n  executor:\n    - {{ adapter: {adapter}, model: m }}\nsecrets: [WORDS_TOKEN]\n"
    );
    if let Some(binary) = binary {
        config.push_str(&format!(
            "adapters:\n  {adapter}:\n    binary: {}\n",
            binary.display()
        ));
    }
    write(&repo.join(".yunta/config.yaml"), &config);
    write(
        &repo.join("wf.yaml"),
        "name: talks\nnodes:\n  - { id: talk, kind: prompt, runner: executor, prompt: \"Say what you do.\" }\n",
    );
    scripts(repo, said);
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "project"]);
}

/// Writes what each CLI prints for a session that opens and says each of
/// `said`, and the mock's script of the same. With no line that ends the
/// session, the stubs linger until the run is cancelled, and the mock hangs.
fn scripts(repo: &std::path::Path, said: &[&str]) {
    let quoted: Vec<String> = said
        .iter()
        .map(|text| serde_json::to_string(text).unwrap())
        .collect();
    let claude: String = quoted
        .iter()
        .enumerate()
        .map(|(n, text)| {
            format!("{{\"type\":\"assistant\",\"message\":{{\"id\":\"msg_{n}\",\"content\":[{{\"type\":\"text\",\"text\":{text}}}]}}}}\n")
        })
        .collect();
    write(
        &repo.join(".claude-stub-lines.jsonl"),
        &format!("{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"sess-words\",\"model\":\"m\"}}\n{claude}"),
    );
    let codex: String = quoted
        .iter()
        .enumerate()
        .map(|(n, text)| {
            format!("{{\"type\":\"item.completed\",\"item\":{{\"id\":\"item_{n}\",\"type\":\"agent_message\",\"text\":{text}}}}}\n")
        })
        .collect();
    write(
        &repo.join(".codex-stub-lines.jsonl"),
        &format!("{{\"type\":\"thread.started\",\"thread_id\":\"thread-words\"}}\n{codex}"),
    );
    let notes: String = quoted
        .iter()
        .enumerate()
        .map(|(n, text)| {
            let after = if n == 0 { 0 } else { SAYS_AGAIN_AFTER_MS };
            format!("      - {{ type: note, text: {text}, after_ms: {after} }}\n")
        })
        .collect();
    write(
        &repo.join("fixture.yaml"),
        &format!(
            "dialect: claude-code\nsessions:\n  - steps:\n{notes}    outcome: {{ type: hang }}\n"
        ),
    );
}
