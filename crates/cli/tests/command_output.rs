//! What the commands a run spawns print: kept by the run, redacted, and
//! never written to the terminal the run's own display owns.

use serde_json::Value;
use yunta_testkit::{run_id_from, runs_root, stderr, stdout, yunta_at, Checkout};

/// A planner that hands over one task, and a loop whose hook and whose
/// task's criterion both print on stdout and stderr.
const NOISY: &str = r#"
name: noisy
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
    hooks:
      before:
        - run: "echo HOOK-SAID-THIS; echo HOOK-SAID-THIS-TOO >&2"
"#;

/// The criterion says the secret before it answers, so what the run keeps
/// of it is what redaction has to hold.
const NOISY_FIXTURE: &str = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks:
              - id: T001
                title: "Make it"
                scope: ["made.txt"]
                criteria:
                  - cmd: "echo CRITERION-SAID-THIS; echo \"token $NOISY_TOKEN\" >&2; test -f made.txt"
    outcome: { type: completed, summary: "planned" }
  - effects:
      - { path: made.txt, content: "made" }
    outcome: { type: completed, summary: "made it" }
"#;

const SECRET: &str = "noisy-secret-value";

fn events(checkout: &Checkout, run_id: &str) -> Vec<Value> {
    let log = std::fs::read_to_string(runs_root(&checkout.home).join(run_id).join("events.jsonl"))
        .expect("the exported event log");
    log.lines()
        .map(|line| serde_json::from_str(line).expect("stored event"))
        .collect()
}

fn object(checkout: &Checkout, run_id: &str, hash: &Value) -> String {
    let hash = hash.as_str().expect("an object hash");
    std::fs::read_to_string(
        runs_root(&checkout.home)
            .join(run_id)
            .join("objects")
            .join(hash),
    )
    .expect("the object the event names")
}

#[test]
fn what_criteria_and_hooks_print_is_the_runs_and_never_the_terminals() {
    let checkout = Checkout::new()
        .with_stubs(vec![("NOISY_TOKEN".to_string(), SECRET.to_string())])
        .config(
            "defaults:\n  isolation: none\nrunners:\n  executor:\n    \
             - { adapter: mock, model: mock-model }\nsecrets: [NOISY_TOKEN]\n",
        )
        .workflow("wf", NOISY)
        .file("fixture.yaml", NOISY_FIXTURE)
        .committed();

    let run = yunta_at!(
        &checkout,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml"
        ]
    );
    let (out, err) = (stdout(&run), stderr(&run));
    assert!(run.status.success(), "stdout: {out}\nstderr: {err}");
    for said in ["CRITERION-SAID-THIS", "HOOK-SAID-THIS", "token "] {
        assert!(
            !out.contains(said) && !err.contains(said),
            "`{said}` reached the terminal:\nstdout: {out}\nstderr: {err}"
        );
    }

    let run_id = run_id_from(&run);
    let events = events(&checkout, &run_id);
    criteria_are_kept(&checkout, &run_id, &events);
    hooks_are_kept(&checkout, &run_id, &events);
    assert!(
        !events
            .iter()
            .any(|event| event.to_string().contains(SECRET)),
        "the secret reaches no event"
    );
}

/// A build that fails the way a compiler does: why on stdout, the secret
/// it was handed on stderr, and a non-zero exit.
const FAILING_BUILD: &str = r#"
name: failing-build
nodes:
  - id: build
    kind: bash
    run: |
      printf 'error[E0425]: cannot find value `x` in this scope\n --> src/lib.rs:3:5\n'
      echo "token $NOISY_TOKEN" >&2
      exit 101
"#;

/// A project with [`FAILING_BUILD`] run once, and the id of that run.
fn failed_build() -> (Checkout, String) {
    let checkout = Checkout::new()
        .with_stubs(vec![("NOISY_TOKEN".to_string(), SECRET.to_string())])
        .config("defaults:\n  isolation: none\nsecrets: [NOISY_TOKEN]\n")
        .workflow("wf", FAILING_BUILD)
        .committed();
    let run = yunta_at!(&checkout, &["run", "wf.yaml"]);
    assert!(!run.status.success(), "the build fails");
    let run_id = run_id_from(&run);
    (checkout, run_id)
}

#[test]
fn a_failed_bash_node_keeps_what_it_printed_redacted_and_quotes_its_last_lines() {
    let (checkout, run_id) = failed_build();
    let events = events(&checkout, &run_id);

    let failed = events
        .iter()
        .find(|event| event["kind"] == "node_failed")
        .expect("the build failed");
    assert_eq!(failed["exited"]["code"], 101);
    let tail: Vec<&str> = failed["exited"]["tail"]
        .as_array()
        .expect("a failed command carries its tail")
        .iter()
        .map(|line| line.as_str().expect("a line"))
        .collect();
    assert_eq!(
        tail,
        vec![
            "error[E0425]: cannot find value `x` in this scope",
            " --> src/lib.rs:3:5",
            "token [redacted]"
        ],
        "stdout, then stderr, with the secret taken out"
    );
    assert_eq!(
        object(&checkout, &run_id, &failed["exited"]["output"]),
        "error[E0425]: cannot find value `x` in this scope\n --> src/lib.rs:3:5\n\
         token [redacted]\n",
        "the whole output is kept, redacted like the log"
    );
    assert!(
        !events
            .iter()
            .any(|event| event.to_string().contains(SECRET)),
        "the secret reaches no event"
    );
}

#[test]
fn status_quotes_the_tail_of_a_failed_command_and_names_where_the_rest_is() {
    let (checkout, run_id) = failed_build();

    let status = yunta_at!(&checkout, &["status", &run_id]);
    let out = stdout(&status);
    assert!(
        out.contains("error[E0425]: cannot find value `x` in this scope")
            && out.contains(" --> src/lib.rs:3:5"),
        "the reason the compiler gave is on the page:\n{out}"
    );
    let objects = runs_root(&checkout.home).join(&run_id).join("objects");
    let named = out
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("whole output: "))
        .unwrap_or_else(|| panic!("the page names where the output is kept:\n{out}"));
    // Shown under `~` when it is under the home the run sees.
    let named = match named.strip_prefix("~/") {
        Some(rest) => checkout.home.join(rest),
        None => std::path::PathBuf::from(named),
    };
    assert!(
        named.starts_with(&objects) && named.is_file(),
        "the page names the file that holds everything the command printed:\n{out}"
    );

    let json = yunta_at!(&checkout, &["status", &run_id, "--json"]);
    let document: Value = serde_json::from_str(&stdout(&json)).expect("one JSON document");
    let build = document["nodes"]
        .as_array()
        .expect("the nodes")
        .iter()
        .find(|node| node["id"] == "build")
        .expect("the build node");
    assert_eq!(build["command_exit"]["code"], 101, "{build}");
    assert_eq!(build["command_exit"]["tail"][1], " --> src/lib.rs:3:5");
}

/// The red pre-check carries its tail and names its output; the green
/// post-check names its output and carries no tail. Both redacted.
fn criteria_are_kept(checkout: &Checkout, run_id: &str, events: &[Value]) {
    let check = |phase: &str| {
        events
            .iter()
            .find(|event| event["kind"] == "criteria_checked" && event["phase"] == phase)
            .unwrap_or_else(|| panic!("a {phase}-check"))
    };

    let red = &check("pre")["results"][0];
    let tail: Vec<&str> = red["tail"]
        .as_array()
        .expect("a red criterion carries its tail")
        .iter()
        .map(|line| line.as_str().expect("a line"))
        .collect();
    assert_eq!(
        tail,
        vec!["CRITERION-SAID-THIS", "token [redacted]"],
        "stdout, then stderr, with the secret taken out"
    );
    assert_eq!(
        object(checkout, run_id, &red["output"]),
        "CRITERION-SAID-THIS\ntoken [redacted]\n",
        "the whole output is kept, redacted like the log"
    );

    let green = &check("post")["results"][0];
    assert_eq!(green["exit_code"], 0);
    assert!(
        green.get("tail").is_none(),
        "a criterion that passed carries no tail: {green}"
    );
    assert!(
        object(checkout, run_id, &green["output"]).contains("CRITERION-SAID-THIS"),
        "what a green criterion printed is kept too"
    );
}

/// The hook that passed names what it printed, and carries no tail.
fn hooks_are_kept(checkout: &Checkout, run_id: &str, events: &[Value]) {
    let hook = events
        .iter()
        .find(|event| event["kind"] == "hook_executed")
        .expect("the hook ran");
    assert_eq!(hook["exit_code"], 0);
    assert!(hook.get("tail").is_none(), "{hook}");
    assert_eq!(
        object(checkout, run_id, &hook["output"]),
        "HOOK-SAID-THIS\nHOOK-SAID-THIS-TOO\n"
    );
}
