//! The past runs: two finished runs of `yunta/fragua` in a repository set up as the
//! design's, the cheapest a finished fragua run can be built the same twice: its `quick`
//! mode, so no gate stops it before `ship`; no lint declared, so it leaves `lint` and
//! `fix-lint` out; one task; and a forge that opens its pull request at once.
//!
//! The repository holds what `yunta pack add` writes when it installs fragua, the
//! reference pack, from a repository holding it at its root; the design's two workflows
//! of its own beside it, `run-tasks` reading a tasks document beside its request; the
//! design's config, each role on Claude Code, then on Codex; and a library each run adds
//! to. The runs freeze another config, every role on the mock adapter, as the design's own
//! run does.
//!
//! Each run is recorded as the design's run is: its suite held until its first session
//! opened, each session held by a word and let go once the clock moved by the time the
//! session works, its `ship` parked, answered as another process answers it, and woken.
//! Each spends and takes its own, so beside the design's run, a third run of fragua, a
//! new run of it says what it usually takes.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use yunta_api::{Remark, Stop};
use yunta_core::port::{
    Forge, ForgeError, ForgeProbe, PolledGate, PublishRequest, PullRequestRef, PullRequestRequest,
};
use yunta_testkit::reference_pack::ReferencePack;
use yunta_testkit::{wait_until_async, Bench};

use super::{id, SmallRun};
use crate::designed::place::Place;
use crate::designed::script::OPENED;

/// Where the repository keeps fragua once installed.
const INSTALLED: &str = ".yunta/packs/yunta/fragua";

/// Where fragua's workflows stand in the repository once installed: where its prompts are
/// read from, and its provenance.
const WORKFLOWS: &str = ".yunta/packs/yunta/fragua/.yunta/workflows";

/// The lint and its one fix round, as the design lists the workflow.
const LINT_FIX: &str = r#"name: lint-fix
description: lint and format, with one fix round
nodes:
  - id: lint
    kind: bash
    invariant: true
    run: "cargo clippy --workspace --all-targets -- -D warnings"
    on_failure: { goto: fix-lint, max_reroutes: 1 }

  - id: fix-lint
    kind: prompt
    runner: mechanical
    context: [{ node-output: { node: lint } }]
    prompt: "Fix exclusively the errors in the report."
    scope: ["crates/**"]

  - id: fmt
    kind: bash
    invariant: true
    depends_on: [lint]
    run: "cargo fmt --all -- --check"
"#;

/// A tasks document a person wrote, run task by task, as the design lists the workflow:
/// what the person asks for, and the document beside it.
const RUN_TASKS: &str = r#"name: run-tasks
description: a tasks document you wrote by hand
inputs:
  goal:
    type: string
    request: true
    description: "What the tasks build"
  tasks:
    type: document
    kind: tasks
    required: true
    description: "Tasks document YAML, relative to the repository root."
nodes:
  - id: implement
    kind: loop
    runner: executor
    until: all_tasks_complete
    invariant: true
    prompt: |
      Read your task from the tasks document. Implement it inside its own scope and
      make every criterion pass; the engine verifies both.

  - id: verify
    kind: bash
    invariant: true
    depends_on: [implement]
    run: |
      cargo fmt --all -- --check
      cargo clippy --workspace --all-targets -- -D warnings
      cargo test --workspace
"#;

/// The repository's config, as the design's repository has it: each role on Claude Code,
/// then on Codex, the suite a run is measured against, and the forge pull requests open on.
const CONFIG: &str = r#"project:
  base_branch: main
runners:
  planner:
    - { adapter: claude-code, model: claude-opus-4-8 }
    - { adapter: codex, model: gpt-6-luna }
  executor:
    - { adapter: claude-code, model: claude-sonnet-4-6 }
    - { adapter: codex, model: gpt-6-luna }
  mechanical:
    - { adapter: claude-code, model: claude-haiku-4-5 }
    - { adapter: codex, model: gpt-6-luna }
  reviewer:
    - { adapter: claude-code, model: claude-sonnet-4-6 }
    - { adapter: codex, model: gpt-6-luna }
  reviewer-alt:
    - { adapter: codex, model: gpt-6-luna }
baseline:
  suite: "cargo test --workspace"
forge:
  github: { repo: eulke/yunta, token_env: YUNTA_DESIGNED_FORGE_TOKEN }
"#;

/// The config the past runs freeze: every role on the mock adapter, the suite and the
/// forge as the repository's.
const ON_THE_MOCK: &str = r#"project:
  base_branch: main
runners:
  planner:
    - { adapter: mock, model: gpt-6-luna }
  executor:
    - { adapter: mock, model: gpt-6-luna }
  mechanical:
    - { adapter: mock, model: gpt-6-luna }
  reviewer:
    - { adapter: mock, model: gpt-6-luna }
  reviewer-alt:
    - { adapter: mock, model: gpt-6-luna }
baseline:
  suite: "cargo test --workspace"
forge:
  github: { repo: eulke/yunta, token_env: YUNTA_DESIGNED_FORGE_TOKEN }
"#;

/// Each session a quick run of fragua opens, in the order it opens them: its node, and the
/// words its prompt holds that no other's does.
const SESSIONS: [(&str, &str); 5] = [
    ("grill", "only a person can decide"),
    ("brief", "Write the brief"),
    ("plan", "Register a tasks document"),
    ("implement", "Build your task as its plan declares it"),
    ("conform", "Hold the work this run built to the plan"),
];

/// One past run: its handle, what it was asked for, the function its one task adds with
/// the line its documentation opens with, each session's tokens in and out and the seconds
/// it works, in [`SESSIONS`]' order, how long `ship` waits for its person, and how long the
/// clock moves before it starts.
struct Past {
    handle: &'static str,
    idea: &'static str,
    adds: (&'static str, &'static str),
    sessions: [(u64, u64, u64); 5],
    ship_waits: u64,
    starts_after: u64,
}

/// The past runs, oldest first. The first starts three hours after the instant the
/// design's run started at, once it ended, and the second an hour and a quarter after the
/// first ended; with the design's run, the median run spends 2.17M tokens and takes 1h07m.
const PAST: [Past; 2] = [
    Past {
        handle: "PAST01",
        idea: "say what a run usually takes before it starts",
        adds: ("usual_spend", "What a run of a workflow usually spends"),
        sessions: [
            (120_000, 6_000, 48),
            (90_000, 5_000, 35),
            (420_000, 18_000, 290),
            (560_000, 24_000, 1_680),
            (130_000, 7_000, 287),
        ],
        ship_waits: 480,
        starts_after: 10_800,
    },
    Past {
        handle: "PAST02",
        idea: "keep a pack's lock beside its vendored tree",
        adds: (
            "lock_beside",
            "Where the lock of a pack stands beside its vendored tree",
        ),
        sessions: [
            (180_000, 8_000, 61),
            (110_000, 6_000, 42),
            (650_000, 27_000, 415),
            (920_000, 38_000, 2_400),
            (220_000, 11_000, 382),
        ],
        ship_waits: 720,
        starts_after: 4_380,
    },
];

/// The model every role of the past runs is on, as the design's run's are.
const MODEL: &str = "gpt-6-luna";

/// The library the repository holds before the runs, which each run adds a function to.
const LIBRARY: &str = "//! What the design's repository builds.\n";

/// The number of the first pull request a past run opens: the repository's pull requests
/// before the design's run's, its 31st.
const FIRST_PULL_REQUEST: u64 = 27;

/// How many of the log's last events a recording that waited in vain says: enough to read
/// the step the run took instead.
const TAIL: usize = 8;

/// The run of [`PAST`] the place's own bench makes: the first, so the place's id is its.
const FIRST: usize = 0;

/// Two finished runs of `yunta/fragua` installed in a repository set up as the design's,
/// each spending other tokens and taking other time, started hours apart.
pub async fn runs() -> SmallRun {
    let installed = fragua_installed().await;
    let mut files: Vec<(String, String)> = vec![
        (".yunta/config.yaml".to_string(), CONFIG.to_string()),
        (
            ".yunta/workflows/lint-fix.yaml".to_string(),
            LINT_FIX.to_string(),
        ),
        (
            ".yunta/workflows/run-tasks.yaml".to_string(),
            RUN_TASKS.to_string(),
        ),
        ("src/lib.rs".to_string(), LIBRARY.to_string()),
    ];
    files.extend(installed);
    let before: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let first = &PAST[FIRST];
    let forge: Arc<dyn Forge> = Arc::new(Opening(AtomicU64::new(FIRST_PULL_REQUEST)));
    let place = Place::of(&id(first.handle), &before, |bench| {
        let workflows = bench.worktree.join(WORKFLOWS);
        bench
            .with_workflow_dir(workflows)
            .with_forge(forge)
            .in_mode("quick")
            .with_inputs(&[("idea", first.idea)])
    });
    let fragua =
        tokio::fs::read_to_string(place.bench.worktree.join(WORKFLOWS).join("fragua.yaml"))
            .await
            .expect("fragua, installed");
    for (at, run) in PAST.iter().enumerate() {
        let beside = (at != FIRST).then(|| {
            place
                .bench
                .beside(&id(run.handle))
                .with_inputs(&[("idea", run.idea)])
        });
        let bench = beside.as_ref().unwrap_or(&place.bench);
        worked(&place, bench, &fragua, run).await;
    }
    SmallRun::of(place)
}

/// `run`, recorded on `bench` in `place`: created once the time before it passed, woken
/// with each session let go in turn, parked on `ship`, approved, and woken to its end.
async fn worked(place: &Place, bench: &Bench, fragua: &str, run: &Past) {
    place.moves(run.starts_after);
    bench
        .create(fragua, &place.fixture(&fixture(run)), ON_THE_MOCK)
        .await;
    place.arm("suite");
    let (_report, ()) = tokio::join!(bench.wake(), sessions(place, bench, run));
    place.moves(run.ship_waits);
    bench
        .answer_parked("approve", None)
        .await
        .expect("`ship` takes its answer");
    bench.wake().await;
}

/// What the recording does while `run`'s first wake drives it: its suite measured once
/// `grill`'s session opened, then each session let go once it opened and the clock moved
/// by the seconds it works.
async fn sessions(place: &Place, bench: &Bench, run: &Past) {
    logged(bench, OPENED, Some("grill")).await;
    wait_until_async(
        || {
            let held = place.holds("suite");
            async move { held }
        },
        || format!("`{}`'s suite never held", run.handle),
    )
    .await;
    place.give("suite");
    logged(bench, "baseline_captured", None).await;
    for ((node, _), (_, _, seconds)) in SESSIONS.iter().zip(run.sessions) {
        logged(bench, OPENED, Some(node)).await;
        place.moves(seconds);
        place.give(&format!("{}-{node}", run.handle));
    }
}

/// Waits until `bench`'s run logged an event of `kind` naming `node`.
async fn logged(bench: &Bench, kind: &str, node: Option<&str>) {
    wait_until_async(
        || {
            let seen = bench.events().iter().any(|event| {
                event.body.kind_name() == kind
                    && event.node_id.as_ref().map(|id| id.as_str()) == node
            });
            async move { seen }
        },
        || {
            let events = bench.events();
            let tail: Vec<String> = events
                .iter()
                .skip(events.len().saturating_sub(TAIL))
                .map(|event| serde_json::to_string(event).expect("an event as the log keeps it"))
                .collect();
            format!(
                "`{}` never logged `{kind}` of {node:?}, and its log ends:\n{}",
                bench.run_id,
                tail.join("\n")
            )
        },
    )
    .await;
}

/// The mock's sessions of `run`: each held by its word, spending its tokens; `plan` hands
/// over one task, which adds to the library a function whose documentation its criterion
/// finds, and `implement` adds it.
fn fixture(run: &Past) -> String {
    let mut fixture = String::from("dialect: claude-code\nsessions:\n");
    for ((node, prompt), (input, output, _)) in SESSIONS.iter().zip(run.sessions) {
        fixture.push_str(&format!(
            "  - match_prompt_contains: \"{prompt}\"\n    model: {MODEL}\n    steps:\n      \
             - {{ type: awaits_word, word: \"@words@/{handle}-{node}\" }}\n      \
             - {{ type: usage, input_tokens: {input}, output_tokens: {output} }}\n",
            handle = run.handle,
        ));
        fixture.push_str(&session_end(node, run));
    }
    fixture
}

/// What the session of `node` does in `run` after its tokens, and how it ends.
fn session_end(node: &str, run: &Past) -> String {
    let (idea, (adds, says)) = (run.idea, run.adds);
    match node {
        "grill" => "      - { type: run_tool, tool: yunta_submit_questions, arguments: { document: \
                    { questions: [] } } }\n    outcome: { type: completed, summary: \"nothing to \
                    ask\" }\n"
            .to_string(),
        "brief" => format!(
            "    effects:\n      - {{ path: \"{{{{run.staging}}}}/brief/brief.md\", content: \
             \"# Brief\\n\\n{idea}.\\n\" }}\n    outcome: {{ type: completed, summary: \"brief \
             written\" }}\n"
        ),
        "plan" => format!(
            "      - type: run_tool\n        tool: yunta_submit_tasks\n        arguments:\n          \
             document:\n            summary: \"{idea}\"\n            description: \"Adds what \
             it takes to {idea}.\"\n            tasks:\n              - id: T001\n                \
             title: \"Add `{adds}`\"\n                description: \"Adds the function.\"\n                \
             scope: [\"src/**\"]\n                changes:\n                  - {{ at: src/lib.rs, \
             what: \"the function\", code: \"pub fn {adds}() -> bool\" }}\n                \
             outcome: \"The library offers `{adds}`\"\n                criteria:\n                  \
             - cmd: \"grep -q '/// {says}' src/lib.rs\"\n                    proves: \"the \
             library documents it\"\n    outcome: {{ type: completed, summary: \"tasks document \
             registered\" }}\n"
        ),
        "implement" => format!(
            "    effects:\n      - {{ path: src/lib.rs, content: \"/// {says}.\\npub fn {adds}() \
             -> bool {{\\n    true\\n}}\\n\" }}\n    outcome: {{ type: completed, \
             summary: \"added `{adds}`\" }}\n"
        ),
        _ => "    outcome: { type: completed, summary: \"the work holds to its plan\" }\n"
            .to_string(),
    }
}

/// The files `yunta pack add` writes into a repository it installs fragua in, each by its
/// path under the repository: the vendored pack and the lock. Fetched from a repository
/// beside it holding the reference pack at its root, named relative to the repository, so
/// the lock reads the same wherever it is written.
async fn fragua_installed() -> Vec<(String, String)> {
    let scratch = tempfile::tempdir().expect("a place to install fragua in");
    let reference = ReferencePack::made(&scratch.path().join("yunta"));
    reference.at_a_root(&scratch.path().join("fragua"));
    let repository = scratch.path().join("repository");
    tokio::fs::create_dir_all(&repository)
        .await
        .expect("the repository's directory");
    yunta_testkit::init_repo(&repository);
    let workspace = crate::sandbox::workspace(&repository, scratch.path());
    let stop = Stop::new();
    let remarks = |_: Remark| {};
    let fetched = workspace
        .packs()
        .fetch("../fragua", false, &stop)
        .await
        .expect("fragua fetched");
    fetched
        .install(&remarks, &stop)
        .await
        .expect("fragua installed");
    let mut files = Vec::new();
    gathered(&repository, &repository.join(INSTALLED), &mut files);
    gathered(
        &repository,
        &repository.join(".yunta/yunta.lock"),
        &mut files,
    );
    files
}

/// Every file at `at` under `repository`, a file or a directory, each by its path under
/// `repository` and with its text, added to `files` in the order of their paths.
fn gathered(repository: &Path, at: &Path, files: &mut Vec<(String, String)>) {
    if at.is_file() {
        let path = at
            .strip_prefix(repository)
            .expect("a file of the repository");
        let text = std::fs::read_to_string(at).expect("a file of the pack");
        files.push((path.display().to_string(), text));
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(at)
        .expect("a directory of the pack")
        .map(|entry| entry.expect("an entry of the pack").path())
        .collect();
    entries.sort();
    for entry in entries {
        gathered(repository, &entry, files);
    }
}

/// The forge the past runs open their pull requests on: each the number after the last's.
struct Opening(AtomicU64);

#[async_trait]
impl Forge for Opening {
    async fn publish(&self, request: &PublishRequest) -> Result<PullRequestRef, ForgeError> {
        Err(ForgeError::Http {
            action: "publish a gate",
            status: 404,
            body: format!("a past run publishes no gate: {}", request.branch),
        })
    }

    async fn poll(&self, gate: &PullRequestRef) -> Result<PolledGate, ForgeError> {
        Err(ForgeError::UnknownGate {
            number: gate.number,
        })
    }

    async fn open_pull_request(
        &self,
        _request: &PullRequestRequest,
    ) -> Result<PullRequestRef, ForgeError> {
        let number = self.0.fetch_add(1, Ordering::SeqCst);
        Ok(PullRequestRef {
            url: format!("https://github.com/eulke/yunta/pull/{number}"),
            number,
        })
    }

    async fn probe(&self) -> Result<ForgeProbe, ForgeError> {
        Ok(ForgeProbe {
            can_push: Some(true),
        })
    }
}
