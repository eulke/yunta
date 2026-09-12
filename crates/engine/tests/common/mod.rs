#![allow(dead_code)]
#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::SeqIdSource;
use yunta_core::{AdapterId, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit::{git, init_repo, Bench, FixedClock, ScriptedInteraction, MOCK_CONFIG};

/// Run ids for everything a test run gives birth to — unique across
/// the binary, so parallel tests never share a run directory.
pub static IDS: SeqIdSource = SeqIdSource::new("minted");

// --- kind: questions ------------------------------------

pub const QUESTIONS_WORKFLOW: &str = r#"
name: ask
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Ask what you need to know before continuing."
    artifacts:
      produces:
        - { name: questions.yaml, kind: questions }
"#;

/// The session [`QUESTIONS_WORKFLOW`]'s `ask` node runs: it hands two
/// questions — one `choice`, one optional `text` — over through
/// `yunta_submit_questions`, and the engine writes `questions.yaml`.
pub const QUESTIONS_FIXTURE: &str = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          name: questions.yaml
          document:
            questions:
              - id: q1
                text: "Which environment?"
                answer_type: choice
                values: [staging, production]
                required: true
              - id: q2
                text: "Any notes?"
                answer_type: text
                required: false
    outcome: { type: completed, summary: "asked" }
"#;

// --- kind: questions → superficie interactiva ------------

/// A test surface that answers questions from a script — `resolve`
/// deliberately returns `None` so these tests prove `ask` alone drives
/// the flow.
pub struct ScriptedAnswers {
    pub answers: Vec<yunta_core::Answer>,
}

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for ScriptedAnswers {
    async fn resolve(
        &self,
        _escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::HumanChoice> {
        None
    }
    async fn ask(
        &self,
        _questions: &yunta_core::QuestionsFile,
        _interactive: bool,
    ) -> Option<yunta_engine::QuestionsReply> {
        Some(yunta_engine::QuestionsReply {
            answers: self.answers.clone(),
            channel: yunta_core::events::Channel::Tty,
            responder: Some("eulke".into()),
        })
    }
}

pub fn answer(id: &str, value: &str) -> yunta_core::Answer {
    yunta_core::Answer {
        id: id.into(),
        value: value.to_string(),
    }
}

/// The most nodes ever started but not yet finished at one point in the
/// log — the scheduler's realized parallelism, read from event order rather
/// than a wall clock. A file barrier in the fan-out nodes holds a batch open
/// together, so this reaches exactly the batch size the scheduler allowed.
pub fn max_open_nodes(events: &[yunta_core::events::StoredEvent]) -> usize {
    use yunta_core::events::EventPayload;
    let mut open = 0i32;
    let mut max = 0i32;
    for event in events {
        match event.payload() {
            Some(EventPayload::NodeStarted(_)) => {
                open += 1;
                max = max.max(open);
            }
            Some(EventPayload::NodeFinished(_) | EventPayload::NodeFailed(_)) => open -= 1,
            _ => {}
        }
    }
    max as usize
}

pub const CONFIG_WITH_BASELINE: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
baseline:
  suite: "cat marker.txt"
"#;

pub const CONFIG_WITH_COVERAGE: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
coverage:
  cmd: "cat coverage.txt"
  threshold: 80.0
"#;

pub const CONFIG_WITH_EXECUTOR: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
skills:
  executors:
    - { name: probe, kind: binary, path: probe.py }
"#;

pub fn write_executable_script(path: &std::path::Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
}

pub const CONFIG_WITH_DENY: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
permissions:
  commands:
    deny: ["*forbidden-marker*"]
"#;

// --- tasks documents handed over through the run tools -----

/// One session that hands `tasks` over as `plan.yaml` through
/// `yunta_submit_tasks` and closes with `summary` — the engine
/// validates the document and writes the file itself. This is a session
/// entry alone, to append after others in a fixture; [`plan_session`]
/// opens a fixture with it.
pub fn tasks_session(tasks: &str, summary: &str) -> String {
    let document: String = tasks
        .lines()
        .map(|line| format!("            {line}\n"))
        .collect();
    format!(
        "  - steps:\n      - type: run_tool\n        tool: yunta_submit_tasks\n\
         \x20       arguments:\n          name: plan.yaml\n          document:\n{document}\
         \x20   outcome: {{ type: completed, summary: {summary} }}\n"
    )
}

/// The fixture a loop test starts from: the run tools its planner
/// submits through, and the planner's own session handing `tasks` over.
/// Executor sessions a test appends land after it, in dispatch order.
pub fn plan_session(tasks: &str) -> String {
    format!(
        "capabilities: {{ run_tools: true }}\nsessions:\n{}",
        tasks_session(tasks, "planned")
    )
}

/// One session posting each `(id, severity, title, location, detail)`
/// through `yunta_post_finding` — the engine derives `findings.yaml`
/// from them when the node closes. An empty slice is a review that
/// finds nothing.
pub fn review_session(findings: &[(&str, &str, &str, &str, &str)]) -> String {
    let steps: String = findings
        .iter()
        .map(|(id, severity, title, location, detail)| {
            format!(
                "      - type: run_tool\n        tool: yunta_post_finding\n        arguments:\n\
                 \x20         id: {id}\n          severity: {severity}\n          title: {title:?}\n\
                 \x20         location: {location:?}\n          detail: {detail:?}\n"
            )
        })
        .collect();
    format!(
        "capabilities: {{ run_tools: true }}\nsessions:\n  - steps:\n{steps}\
         \x20   outcome: {{ type: completed, summary: \"reviewed\" }}\n"
    )
}

// --- concurrency: N in loop nodes -----------------------

pub const CONCURRENCY_CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
"#;

/// An 8-independent-task document: no `depends_on` between any of them, each
/// with its own disjoint scope (`out-N.txt`) so `tasks::register`
/// accepts it as a legal batch of fully parallelizable work.
pub fn task_yaml(id: &str, title: &str, scope: &str, criterion: &str) -> String {
    format!(
        "  - id: {id}\n    title: \"{title}\"\n    scope: [\"{scope}\"]\n    criteria:\n      - cmd: \"{criterion}\"\n"
    )
}

pub fn eight_independent_tasks() -> String {
    let mut yaml = String::from("tasks:\n");
    for n in 1..=8 {
        yaml.push_str(&format!(
            "  - id: task-{n}\n    title: \"Write out-{n}\"\n    scope: [\"out-{n}.txt\"]\n    criteria:\n      - cmd: \"test -f out-{n}.txt\"\n"
        ));
    }
    yaml
}

pub fn concurrency_workflow(concurrency: u32) -> String {
    format!(
        r#"
name: eight-tasks
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces:
        - {{ name: plan.yaml, kind: tasks }}
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    concurrency: {concurrency}
    prompt: "Read your task from the tasks document and implement it."
"#
    )
}

/// One mock session per task of [`eight_independent_tasks`],
/// matched by its own id (never by call order — concurrent dispatch
/// races several `spawn()` calls at once), behind the planner's own
/// session.
pub fn eight_tasks_fixture() -> String {
    let mut yaml = plan_session(&eight_independent_tasks());
    for n in 1..=8 {
        yaml.push_str(&format!(
            "  - match_prompt_contains: \"task-{n}\"\n    effects:\n      - {{ path: out-{n}.txt, content: \"{n}\" }}\n    outcome: {{ type: completed, summary: \"did task-{n}\" }}\n"
        ));
    }
    yaml
}

/// Commit subjects on `worktree`'s current branch, oldest first, excluding
/// the `init_repo` seed commit.
pub fn commit_subjects(worktree: &std::path::Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["log", "--format=%s", "--reverse"])
        .current_dir(worktree)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| *line != "initial")
        .map(str::to_string)
        .collect()
}

// --- scope_expansion ------------------------------------

/// A loop node declaring `scope_expansion:` — `within` is only rendered
/// when the caller passes something, so `rules`-mode tests can still omit
/// it when a test wants an empty ceiling.
pub fn scope_expansion_workflow(mode: &str, within: &[&str], max_per_run: Option<u32>) -> String {
    let within_line = if within.is_empty() {
        String::new()
    } else {
        let items = within
            .iter()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("      within: [{items}]\n")
    };
    let cap_line = max_per_run
        .map(|n| format!("      max_per_run: {n}\n"))
        .unwrap_or_default();
    format!(
        r#"
name: scope-expansion
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces:
        - {{ name: plan.yaml, kind: tasks }}
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
    scope_expansion:
      mode: {mode}
{within_line}{cap_line}"#
    )
}

/// The same loop shape with no `scope_expansion:` key at all — the
/// default (an absent block behaves exactly like `mode: deny` with no
/// `within`/`max_per_run`) — proving that default is really live, not
/// just documented.
pub fn no_scope_expansion_workflow() -> String {
    r#"
name: scope-expansion-default
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces:
        - { name: plan.yaml, kind: tasks }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#
    .to_string()
}

pub fn findings_posted(
    events: &[yunta_core::events::StoredEvent],
) -> Vec<&yunta_core::events::Finding> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::FindingPosted(p)) => Some(&p.finding),
            _ => None,
        })
        .collect()
}

// --- escalación de scope expansion → gate real ---------

/// One `ask`-mode attempt that writes a.txt (in scope), b.txt (outside)
/// and the request file asking for b.txt.
pub fn requesting_session(task_id: &str) -> String {
    let request_yaml = "paths:\n  - b.txt\nreason: \"adjacent fix in b.txt\"\nproposed_criterion:\n  cmd: \"test -f nonexistent-marker\"\n";
    format!(
        "  - match_prompt_contains: {task_id:?}\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: b.txt, content: \"b\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: asked }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    )
}

// --- context: -----------------------------------------------------

/// A single `prompt` node named `ask` declaring `context_yaml` verbatim
/// under `context:`. `runner: executor` matches `MOCK_CONFIG`'s own mock
/// candidate.
pub fn context_workflow(context_yaml: &str) -> String {
    format!(
        "name: ctx\nnodes:\n  - id: ask\n    kind: prompt\n    runner: executor\n    prompt: \"Do the thing.\"\n    context:\n{context_yaml}"
    )
}

/// The one `context_assembled` event's `sources`, for the given node.
pub fn context_sources(
    events: &[yunta_core::events::StoredEvent],
    node: &str,
) -> Vec<yunta_core::events::ContextSourceRef> {
    events
        .iter()
        .find_map(|e| match (&e.node_id, e.payload()) {
            (Some(n), Some(yunta_core::events::EventPayload::ContextAssembled(p)))
                if n.as_str() == node =>
            {
                Some(p.sources.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no context_assembled event found for node `{node}`"))
}

/// Confirms a resolved source is genuinely replayable: the object the
/// run stored under `objects/<content_hash>` exists and its own hash
/// matches what the event recorded — reconstructing it never needs to
/// re-run the command, re-read the original path outside the snapshot,
/// or touch the network.
pub fn assert_materialized(
    run_dir: &std::path::Path,
    source: &yunta_core::events::ContextSourceRef,
) {
    let path = run_dir.join("objects").join(source.content_hash.as_str());
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("materialized file missing at {path:?}: {e}"));
    assert_eq!(
        yunta_core::sha256_hex(&bytes),
        source.content_hash,
        "materialized content must hash to exactly what the event recorded"
    );
}

// --- ensamblado estable-primero --------------------------------

pub fn stable_first_workflow(volatile_command_output: &str) -> String {
    format!(
        "name: stable-first\nnodes:\n  - id: grill\n    kind: prompt\n    runner: executor\n    prompt: \"Write the brief.\"\n    artifacts:\n      produces: [brief.md]\n  - id: plan\n    kind: prompt\n    runner: executor\n    depends_on: [grill]\n    prompt: \"Plan from context.\"\n    context:\n      - command: \"echo {volatile_command_output}\"\n      - artifact: {{ node: grill, name: brief.md }}\n      - files: [\"stable.txt\"]\n"
    )
}

pub async fn run_stable_first(
    bench: &Bench,
    volatile_command_output: &str,
) -> (
    yunta_core::events::ContextSourceRef,
    yunta_core::events::ContextSourceRef,
    BTreeMap<String, yunta_core::ContentHash>,
) {
    std::fs::write(bench.worktree.join("stable.txt"), "STABLE-CONTENT\n").unwrap();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/brief.md\", content: \"FIXED-BRIEF-CONTENT\" }}\n    outcome: {{ type: completed, summary: grilled }}\n  - match_prompt_contains: \"{volatile_command_output}\"\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display(),
    );
    let workflow = stable_first_workflow(volatile_command_output);

    let (terminal, _state) = bench.run(&workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let payload = events
        .iter()
        .find_map(|e| match (&e.node_id, e.payload()) {
            (Some(n), Some(yunta_core::events::EventPayload::ContextAssembled(p)))
                if n.as_str() == "plan" =>
            {
                Some(p.clone())
            }
            _ => None,
        })
        .expect("context_assembled event for `plan`");

    let stable = payload
        .sources
        .iter()
        .find(|s| s.kind == "files")
        .unwrap()
        .clone();
    let run_stable = payload
        .sources
        .iter()
        .find(|s| s.kind == "artifact")
        .unwrap()
        .clone();
    (stable, run_stable, payload.segment_hashes)
}

// --- the org layer resolves from installed knowledge packs ------

/// One installed org knowledge pack under the worktree's own
/// `.yunta/packs/` — the vendored shape `pack add` produces, built
/// directly on disk (same convention as `catalog.rs`'s fixtures).
pub fn write_org_pack(worktree: &Path, publisher: &str, name: &str, files: &[(&str, &str)]) {
    let pack_dir = worktree.join(".yunta/packs").join(publisher).join(name);
    std::fs::create_dir_all(pack_dir.join("knowledge")).unwrap();
    std::fs::write(
        pack_dir.join("pack.yaml"),
        format!(
            "name: {name}\npublisher: {publisher}\nversion: 1.0.0\ndeclares:\n  \
             permissions: read-only\ncontents:\n  knowledge: [knowledge/]\n"
        ),
    )
    .unwrap();
    for (file, content) in files {
        std::fs::write(pack_dir.join("knowledge").join(file), content).unwrap();
    }
}

// --- HumanInteraction — gates ----------------------------------

/// A never-refuses `on_failure.goto` corrective node whose *second*
/// attempt actually fixes what its first attempt didn't — so a human
/// authorizing exactly one extra retry at the exhausted-reroutes gate is
/// what turns this workflow from perpetually failing into finished.
pub const HOPELESS_UNTIL_RETRIED_WORKFLOW: &str = r#"
name: hopeless-until-retried
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "Try to fix it."
"#;

pub const HOPELESS_UNTIL_RETRIED_FIXTURE: &str = r#"
sessions:
  - outcome: { type: completed, summary: "did nothing useful" }
  - effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "actually fixed it this time" }
"#;

// --- gate interno genérico (message/options/on) --------

/// Resolves gates from a scripted sequence, one per call — `None` once
/// the script runs out (so an unexpected extra ask degrades to pause
/// instead of hanging a test).
pub struct SequencedInteraction {
    choices: std::sync::Mutex<std::collections::VecDeque<yunta_core::events::HumanChoice>>,
}

impl SequencedInteraction {
    pub fn choosing(options: &[&str]) -> Self {
        Self {
            choices: std::sync::Mutex::new(
                options
                    .iter()
                    .map(|option| yunta_core::events::HumanChoice {
                        option: (*option).into(),
                        by: "lead".into(),
                        free_text: None,
                    })
                    .collect(),
            ),
        }
    }
}

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for SequencedInteraction {
    async fn resolve(
        &self,
        _escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::HumanChoice> {
        self.choices.lock().unwrap().pop_front()
    }
}

pub const INTERNAL_GATE_WORKFLOW: &str = r#"
name: internal-gate
nodes:
  - id: plan
    kind: bash
    run: "sh -c 'echo run >> plan-runs.txt'"
  - id: approve
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Approve the plan?"
    options: [aprobar, ajustar]
    on: { ajustar: plan }
  - id: ship
    kind: bash
    depends_on: [approve]
    run: "true"
"#;

pub fn plan_run_count(worktree: &std::path::Path) -> usize {
    std::fs::read_to_string(worktree.join("plan-runs.txt"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

// --- run token budget (limits.max_tokens_per_run) ---------------

pub const BUDGET_CONFIG: &str = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_tokens_per_run: 100
"#;

/// A compliant session can never push the run past its cap — its own
/// equal-share `Budget` (etapa 3) stops it first — so the run-level
/// check's real scenario is an *overshoot*: one usage burst blows both
/// the session share and the whole run cap at once, the session is
/// killed, the node fails, and its `on_failure` re-route asks the
/// scheduler for more work while `spent >= cap`.
pub const BUDGET_WORKFLOW: &str = r#"
name: budget
nodes:
  - id: first
    kind: prompt
    runner: executor
    prompt: "Do the first thing."
    on_failure: { goto: fix, max_reroutes: 2 }
  - id: fix
    kind: prompt
    runner: executor
    prompt: "Fix it."
"#;

/// Session 1 bursts 200 tokens against a share of 50 (cap 100 across 2
/// nodes); sessions 2 and 3 (the corrective, then `first`'s retry) only
/// ever run if a human lets the run continue past the cap.
pub const BUDGET_FIXTURE: &str = r#"
sessions:
  - steps:
      - { type: usage, input_tokens: 150, output_tokens: 50 }
    outcome: { type: completed, summary: "spent a lot" }
  - outcome: { type: completed, summary: "fixed" }
  - outcome: { type: completed, summary: "did it" }
"#;

// --- limits.max_loop_iterations ------------------------

/// Three sequential tasks at concurrency 1 need four loop iterations
/// (one per batch plus the closing empty-batch check) — a cap of 2 trips
/// mid-document.
pub const LOOP_CAP_WORKFLOW: &str = r#"
name: loop-cap
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces:
        - { name: plan.yaml, kind: tasks }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;

pub const LOOP_CAP_CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_loop_iterations: 2
"#;

/// The loop-cap run's sessions: the planner hands a chain of three
/// tasks — each depending on the one before, so they can only run one
/// batch at a time — over through the run tools, and one executor
/// session per task follows, in dispatch order.
pub fn loop_cap_fixture() -> String {
    const CHAIN: [(&str, &str); 3] = [("T001", "a"), ("T002", "b"), ("T003", "c")];

    let mut tasks = String::from("tasks:\n");
    let mut previous: Option<&str> = None;
    for (task, file) in CHAIN {
        tasks.push_str(&task_yaml(
            task,
            file,
            &format!("{file}.txt"),
            &format!("test -f {file}.txt"),
        ));
        if let Some(previous) = previous {
            tasks.push_str(&format!("    depends_on: [{previous}]\n"));
        }
        previous = Some(task);
    }

    let mut yaml = plan_session(&tasks);
    for (task, file) in CHAIN {
        yaml.push_str(&format!(
            "  - effects:\n      - {{ path: {file}.txt, content: \"{file}\" }}\n    outcome: {{ type: completed, summary: \"did {task}\" }}\n"
        ));
    }
    yaml
}

// --- limits.inline_context_bytes -------------------------

/// A ~60-byte file source: inlined under the reference default (32000),
/// referenced by pointer when the configured threshold is below it.
pub const INLINE_CONTEXT_WORKFLOW: &str = r#"
name: inline-context
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Use the context above."
    context:
      - files: ["notes.txt"]
"#;

// --- session events (agent_session_opened / agent_message) ------------

pub const SESSION_EVENTS_WORKFLOW: &str = r#"
name: session-events
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#;

// --- skills chain (resolution → SessionRequest → degradation) ---------

pub const SKILLS_CONFIG: &str = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
skills:
  paths: [.yunta/skills]
  always: [conventions]
"#;

pub const SKILLS_WORKFLOW: &str = r#"
name: skilled
nodes:
  - id: work
    kind: prompt
    runner: executor
    skills: [grill]
    prompt: "Do the thing."
"#;

pub fn install_skill(worktree: &std::path::Path, name: &str) {
    let dir = worktree.join(".yunta/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
}

/// Runs one workflow with a hand-held mock so the test can ask it what
/// skills each spawn carried.
pub async fn run_with_recording_mock(
    bench: &Bench,
    workflow_yaml: &str,
    fixture_yaml: &str,
    config_yaml: &str,
) -> (RunTerminal, yunta_engine::RunState, Arc<MockAdapter>) {
    let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_norway::from_str(config_yaml).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let adapter = Arc::new(MockAdapter::from_yaml(fixture_yaml).unwrap());
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), adapter.clone());
    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    (report.terminal, report.state, adapter)
}

// --- on_finish.distill — deterministic knowledge distillation ---------

pub const DISTILL_WORKFLOW: &str = r#"
name: distiller
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{run.dir}}/artifacts/plan.md."
    artifacts:
      produces: [plan.md]
on_finish:
  - distill: [plan.md]
"#;

pub fn distill_fixture(artifacts_dir: &std::path::Path) -> String {
    format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.md", content: "DISTILLED-MARKER: the durable decision\n" }}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        artifacts = artifacts_dir.display()
    )
}

// --- on_interrupt: resume_session -------------------------------------

/// Crafts an interrupted run: `run_created` + a `node_started` (and
/// optionally an open `agent_session_opened`) with no terminal event —
/// exactly what a mid-session crash leaves — then resumes it with a
/// recording mock.
pub async fn resume_orphan_with_mock(
    workflow_yaml: &str,
    fixture_yaml: &str,
    orphan_session: Option<&str>,
) -> (
    RunTerminal,
    Vec<yunta_core::events::StoredEvent>,
    Arc<MockAdapter>,
) {
    let bench = Bench::new();
    let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let emit = |node: &str, payload: yunta_core::events::EventPayload| {
        bench
            .storage
            .append(
                &yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some(node.into()),
                    payload,
                },
                &yunta_core::SystemClock,
            )
            .unwrap();
    };
    emit(
        "work",
        yunta_core::events::EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload {
            attempt: 1,
        }),
    );
    if let Some(session_id) = orphan_session {
        emit(
            "work",
            yunta_core::events::EventPayload::AgentSessionOpened(
                yunta_core::events::AgentSessionOpenedPayload {
                    session_id: session_id.into(),
                    agent: None,
                    model: Some("mock-model".into()),
                    capabilities: yunta_core::Capabilities::default(),
                },
            ),
        );
    }

    let adapter = Arc::new(MockAdapter::from_yaml(fixture_yaml).unwrap());
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), adapter.clone());
    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &bench.runs_root.join(bench.run_id.as_str()),
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    let _ = run_dir;
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    (report.terminal, events, adapter)
}

pub const RESUME_WORKFLOW: &str = r#"
name: resumable
nodes:
  - id: work
    kind: prompt
    runner: executor
    on_interrupt: resume_session
    prompt: "Do the thing."
"#;
