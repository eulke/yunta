//! The behaviours every adapter keeps, one case each. A case asserts through
//! `Subject` alone, so one body runs against claude-code, codex and the mock:
//! a fact a subject cannot express fails for it, and is never skipped. The
//! process cases need a live CLI process, so only the real subjects expand them.

use std::time::Duration;

use futures::StreamExt;
use yunta_core::agent::AgentDefinition;
use yunta_core::events::{FailureCause, RefusalCause};
use yunta_core::fence::Coverage;
use yunta_core::port::{AgentEvent, PermissionProfile, ProbeReport};
use yunta_core::port::{RunToolsEndpoint, SessionRequest};
use yunta_core::rail::RailEvent;
use yunta_core::{sha256_hex, AdapterError, AdapterSettings, CliConfig, Effort, FenceLevel};
use yunta_core::{ModelName, RunTool};
use yunta_testkit_core::adapter::request;

use super::subject::{Outcome, Played, Script, Step, Subject, STOP_TURN};

/// The timeout, in seconds, every hook the adapters install carries. The
/// adapters' own constant is crate-private, so the contract repeats it here.
const HOOK_TIMEOUT_SECS: u64 = 10;

/// A budget timeout no CLI picks by default, so the stop hook must carry it.
const BUDGET_SECS: u64 = 1500;

/// A fresh worktree and the baseline request a case changes one field of.
pub(super) fn fresh() -> (tempfile::TempDir, SessionRequest) {
    let dir = tempfile::tempdir().unwrap();
    let req = request(dir.path().to_path_buf());
    (dir, req)
}

/// A script of `steps` that ends with `outcome`.
fn script(steps: Vec<Step>, outcome: Outcome) -> Script {
    Script {
        model: None,
        steps,
        outcome,
    }
}

/// A script of `steps` that completes.
fn completed(steps: Vec<Step>) -> Script {
    script(steps, Outcome::Completed("ok".to_string()))
}

/// Plays `steps` to completion in a fresh worktree, after `edit` changed the request.
async fn run(
    subject: &dyn Subject,
    steps: Vec<Step>,
    edit: impl FnOnce(&mut SessionRequest),
) -> Played {
    let (dir, mut req) = fresh();
    edit(&mut req);
    subject.play(dir.path(), req, &completed(steps)).await
}

/// The usage a stream reports, as one `(input, output, cached)` per event.
fn usages(events: &[AgentEvent]) -> Vec<(Option<u64>, Option<u64>, Option<u64>)> {
    let usage = |event: &AgentEvent| match *event {
        AgentEvent::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
        } => Some((input_tokens, output_tokens, cached_input_tokens)),
        _ => None,
    };
    events.iter().filter_map(usage).collect()
}

/// Each message a CLI fails with ends its session at the cause its classifier table gives it.
pub async fn failure_causes_follow_the_classifier_table(subject: &dyn Subject) {
    let table = [
        (
            "401 Unauthorized",
            FailureCause::Refused(RefusalCause::Authentication),
        ),
        (
            "Invalid API key. Run /login",
            FailureCause::Refused(RefusalCause::Authentication),
        ),
        (
            "getaddrinfo ENOTFOUND host",
            FailureCause::Refused(RefusalCause::Unreachable),
        ),
        (
            "stream ended unexpectedly",
            FailureCause::Refused(RefusalCause::Unreachable),
        ),
    ];
    for (message, expected) in table {
        let (dir, req) = fresh();
        let failed = script(Vec::new(), Outcome::Failed(message.to_string()));
        let played = subject.play(dir.path(), req, &failed).await;
        let Some(AgentEvent::Failed { cause, .. }) = played.events.last() else {
            panic!("`{message}` ends in Failed: {:?}", played.events);
        };
        assert_eq!(cause, &expected, "`{message}`");
    }
}

/// A run tool a session calls is logged under its canonical name, whichever CLI named it.
pub async fn same_run_tool_call_is_logged_under_one_name(subject: &dyn Subject) {
    let steps = vec![Step::RunToolCalled(RunTool::CheckArtifact)];
    let played = run(subject, steps, |_| {}).await;
    let logged = played.events.iter().any(|event| {
        matches!(event, AgentEvent::ToolUse { name: tool, .. } if tool == "yunta_check_artifact")
    });
    assert!(logged, "{}: {:?}", subject.name(), played.events);
}

/// Each permission profile gives the same sandbox mode and fence coverage under every CLI.
pub async fn profile_gives_the_same_mode_and_coverage(subject: &dyn Subject) {
    for (profile, mode) in [
        (PermissionProfile::ReadOnly, "read-only"),
        (PermissionProfile::Edit, "workspace-write"),
        (PermissionProfile::Full, "danger-full-access"),
    ] {
        let (dir, mut req) = fresh();
        req.permissions = profile;
        let roots = vec![dir.path().to_path_buf()];
        let played = subject.play(dir.path(), req, &completed(Vec::new())).await;
        let expected = match profile {
            PermissionProfile::ReadOnly => Coverage::Exact,
            PermissionProfile::Edit => Coverage::WidenedToRoots { roots },
            PermissionProfile::Full => Coverage::ToolsOnly,
        };
        let Some(AgentEvent::SessionOpened { fence, .. }) = played.events.first() else {
            panic!("a session opens first");
        };
        assert_eq!(fence.as_ref(), Some(&expected), "{profile:?}");
        let sandbox = subject.read_back(&played.argv).sandbox;
        assert_eq!(sandbox.as_deref(), Some(mode), "{profile:?}");
    }
}

/// A launch that cannot reach the rail is refused before it writes or mounts anything.
pub async fn a_launch_without_a_hook_opens_nothing_and_stages_nothing(subject: &dyn Subject) {
    for unset in ["command", "endpoint"] {
        let (dir, mut req) = fresh();
        let skill = dir.path().join("skills-src/grill");
        tokio::fs::create_dir_all(&skill).await.unwrap();
        req.skills = vec![skill];
        if unset == "command" {
            req.rail_command = None;
        } else {
            req.rail = None;
        }
        let adapter = subject.adapter(AdapterSettings::default());
        assert!(adapter.staged_paths(&req).is_empty(), "{unset}");
        let refused = adapter.spawn(req).await.err().expect("a session opened");
        let text = yunta_core::describe(&refused);
        let unbuildable = matches!(refused, AdapterError::FenceUnbuildable { .. });
        assert!(unbuildable, "{unset}: {text}");
        let roots = [".claude", ".agents"];
        let mounted = roots.iter().any(|root| dir.path().join(root).exists());
        assert!(!mounted, "{unset} left a mount");
    }
}

/// A launch installs its write and stop hooks, and the after-write hook when formats are on.
pub async fn launch_installs_the_hooks_it_needs(subject: &dyn Subject) {
    for formats in [false, true] {
        let (dir, mut req) = fresh();
        req.formats = formats;
        req.budget.timeout = Some(Duration::from_secs(BUDGET_SECS));
        let stop = RunToolsEndpoint::call_timeout(&req.budget).as_secs();
        let write = (RailEvent::BeforeWrite, HOOK_TIMEOUT_SECS);
        let mut expected = vec![write, (RailEvent::BeforeStop, stop)];
        if formats {
            expected.push((RailEvent::AfterWrite, HOOK_TIMEOUT_SECS));
        }
        let played = subject.play(dir.path(), req, &completed(Vec::new())).await;
        let hooks = subject.read_back(&played.argv).hooks;
        let same = hooks.len() == expected.len() && expected.iter().all(|h| hooks.contains(h));
        assert!(same, "expected {expected:?}, got {hooks:?}");
    }
}

/// A usage the CLI reports is counted once, and a count it leaves out stays absent.
pub async fn tokens_are_counted_once_and_absent_stays_absent(subject: &dyn Subject) {
    let counted = Step::Usage {
        input: 10,
        output: 4,
        cached: Some(2),
    };
    let played = run(subject, vec![counted], |_| {}).await;
    let reported = usages(&played.events);
    assert_eq!(reported, [(Some(10), Some(4), Some(2))]);
    let absent = Step::Usage {
        input: 40,
        output: 2,
        cached: None,
    };
    let played = run(subject, vec![absent], |_| {}).await;
    let reported = usages(&played.events);
    assert_eq!(reported, [(Some(40), Some(2), None)]);
}

/// A session opens reporting the model its CLI names in its stream, not the one requested.
pub async fn session_reports_the_model_the_cli_reports(subject: &dyn Subject) {
    let mut reporting = completed(Vec::new());
    reporting.model = Some("contract-reported".to_string());
    let (dir, mut req) = fresh();
    req.model = Some("contract-requested".parse().unwrap());
    let played = subject.play(dir.path(), req, &reporting).await;
    let Some(AgentEvent::SessionOpened { model, .. }) = played.events.first() else {
        panic!("a session opens first");
    };
    // A stream that names no model, as Codex's does, leaves the session without one.
    assert!(
        matches!(
            model.as_ref().map(ModelName::as_str),
            None | Some("contract-reported")
        ),
        "{}: {model:?}",
        subject.name()
    );
}

/// A skill is mounted where its CLI finds it while the session lives, and taken back on release.
pub async fn skills_are_staged_and_taken_back(subject: &dyn Subject) {
    let (dir, mut req) = fresh();
    let skill = dir.path().join("skills-src/grill");
    tokio::fs::create_dir_all(&skill).await.unwrap();
    req.skills = vec![skill.clone()];
    let adapter = subject.adapter(AdapterSettings::default());
    let staged = adapter.staged_paths(&req);
    assert_eq!(staged.len(), 1, "{staged:?}");
    let link = dir.path().join(&staged[0]);
    let top = dir.path().join(staged[0].components().next().unwrap());
    let mut session = adapter.spawn(req).await.unwrap();
    let target = tokio::fs::read_link(&link).await.unwrap();
    assert_eq!(target, skill, "linked while the session lives");
    session.events().count().await;
    session.release().await.unwrap();
    let gone = tokio::fs::symlink_metadata(&link).await.is_err() && !top.exists();
    assert!(gone, "the mount outlived its session");
}

/// A named agent's instructions reach the CLI beside its own prompt.
pub async fn a_named_agent_reaches_the_cli(subject: &dyn Subject) {
    let agent = AgentDefinition {
        name: "benito".into(),
        instructions: "Review like Benito.".into(),
    };
    // The instructions are read from the file the CLI is handed by path, so
    // the directory that holds it lives until the arguments are read back.
    let (dir, mut req) = fresh();
    req.agent = Some(agent);
    let played = subject.play(dir.path(), req, &completed(Vec::new())).await;
    let asks = subject.read_back(&played.argv);
    let text = asks.instructions.unwrap_or_default();
    assert!(text.contains("Review like Benito."), "{text:?}");
}

/// A session asked for an effort passes it to its CLI; one asked for none leaves it there.
pub async fn effort_reaches_the_cli_or_is_left_to_it(subject: &dyn Subject) {
    let asked = run(subject, Vec::new(), |req| req.effort = Some(Effort::Low)).await;
    let effort = subject.read_back(&asked.argv).effort;
    assert_eq!(effort.as_deref(), Some(Effort::Low.as_str()));
    let unasked = run(subject, Vec::new(), |_| {}).await;
    assert_eq!(subject.read_back(&unasked.argv).effort, None);
}

/// An isolated session loads none of the person's own configuration; an inheriting one loads it.
pub async fn isolated_session_loads_no_user_config(subject: &dyn Subject) {
    for (cli_config, isolated) in [(CliConfig::Isolated, true), (CliConfig::Inherit, false)] {
        let played = run(subject, Vec::new(), |req| req.cli_config = cli_config).await;
        let reads = subject.read_back(&played.argv).isolated;
        assert_eq!(reads, isolated, "{cli_config:?}");
    }
}

/// The network opens only for a workspace-write session that asked for it.
pub async fn network_opens_only_under_workspace_write(subject: &dyn Subject) {
    for (profile, network, opens) in [
        (PermissionProfile::Edit, true, true),
        (PermissionProfile::Edit, false, false),
        (PermissionProfile::ReadOnly, true, false),
        (PermissionProfile::Full, true, false),
    ] {
        let played = run(subject, Vec::new(), |req| {
            req.permissions = profile;
            req.network = network;
        })
        .await;
        let reads = subject.read_back(&played.argv).network;
        assert_eq!(reads, opens, "{profile:?}, network {network}");
    }
}

/// A fence built on tool calls comes with a rail codec, and no other fence has one.
pub async fn fence_level_and_rail_codec_agree(subject: &dyn Subject) {
    let adapter = subject.adapter(AdapterSettings::default());
    let tool_calls = adapter.capabilities().fence == FenceLevel::ToolCalls;
    assert_eq!(
        tool_calls,
        adapter.rail_codec().is_some(),
        "{}",
        subject.name()
    );
}

/// The kinds of line a CLI prints that the adapter does not read are
/// tolerated: the stream goes on and ends cleanly.
pub async fn an_unknown_stream_line_is_tolerated_and_named(subject: &dyn Subject) {
    let steps = vec![Step::UnknownLine, Step::Note("still here".to_string())];
    let played = run(subject, steps, |_| {}).await;
    let noted = played
        .events
        .iter()
        .any(|event| matches!(event, AgentEvent::Note { text } if text == "still here"));
    // A failure is terminal, so a stream that ends in Completed never failed.
    let ended = matches!(played.events.last(), Some(AgentEvent::Completed { .. }));
    assert!(noted && ended, "{:?}", played.events);
}

/// A line the CLI prints before its session opens fails the session, and the stream ends there.
pub async fn a_line_before_the_session_opens_fails_it(subject: &dyn Subject) {
    let steps = vec![Step::BeforeOpen("hi".to_string())];
    let played = run(subject, steps, |_| {}).await;
    let Some(AgentEvent::Failed { error, .. }) = played.events.first() else {
        panic!("{:?}", played.events);
    };
    let text = &error.message;
    assert!(text.contains("before"), "{text}");
    assert_eq!(played.events.len(), 1, "{:?}", played.events);
}

/// A session whose process dies ends its stream with no terminal event: the adapter invents none.
pub async fn a_crashed_session_ends_the_stream_with_no_terminal_event(subject: &dyn Subject) {
    let (dir, req) = fresh();
    let crash = script(Vec::new(), Outcome::Crash);
    let played = subject.play(dir.path(), req, &crash).await;
    let opened_only = matches!(played.events.as_slice(), [AgentEvent::SessionOpened { .. }]);
    assert!(opened_only, "{:?}", played.events);
}

/// Settings that do not read refuse the session before anything opens, and say what they are.
pub async fn a_session_never_opens_under_unreadable_settings(subject: &dyn Subject) {
    let (_dir, req) = fresh();
    let raw = serde_json::from_str(r#"{"sandbox": "no-such-mode"}"#).unwrap();
    let settings = AdapterSettings {
        adapter_settings: Some(raw),
        ..Default::default()
    };
    let Err(error) = subject.adapter(settings).spawn(req).await else {
        panic!("a session opened under unreadable settings");
    };
    let text = yunta_core::describe(&error);
    let named = text.contains("cannot read its") && text.contains("no-such-mode");
    assert!(named, "{text}");
}

/// A setting the adapter does not read fails the probe, naming the key and the settings it reads.
pub async fn an_unknown_adapter_setting_is_reported_by_probe(subject: &dyn Subject) {
    let mut extra = serde_json::Map::new();
    extra.insert("sandbx".to_string(), serde_json::Value::from("read-only"));
    let settings = AdapterSettings {
        adapter_settings: Some(extra),
        ..Default::default()
    };
    let error = subject.adapter(settings).probe().await.unwrap_err();
    let text = yunta_core::describe(&error);
    let words = ["sandbx", "adapter_settings", "sandbox"];
    let named = words.iter().all(|word| text.contains(*word));
    assert!(named, "{text}");
}

/// Two refused stops in a row, over two states of the work, keep the session going to its end.
pub async fn two_refused_stops_in_a_row_keep_the_session_alive(subject: &dyn Subject) {
    let reasons = ["the unit tests are red", "the linter reports a warning"];
    let steps = reasons.map(|reason| Step::StopRefused {
        reason: reason.into(),
    });
    let played = run(subject, steps.into(), |_| {}).await;
    let turns: Vec<&str> = played
        .events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Note { text } => text.strip_prefix(STOP_TURN),
            _ => None,
        })
        .collect();
    let ended = matches!(played.events.last(), Some(AgentEvent::Completed { .. }));
    let kept = turns == reasons && ended;
    let adapter = subject.adapter(AdapterSettings::default());
    let declared = adapter.capabilities().stop_refusals;
    assert_eq!(declared, kept, "{}: {:?}", subject.name(), played.events);
}

/// The probe reports the version the subject's CLI prints, and the CLI is healthy.
pub async fn probe_reports_the_stub_version(subject: &dyn Subject) {
    let adapter = subject.adapter(AdapterSettings::default());
    let report = adapter.probe().await.unwrap();
    let ProbeReport::Healthy { version } = report else {
        panic!("{report:?}");
    };
    let text = version.unwrap_or_default();
    assert!(text.contains(subject.version()), "{text}");
}

/// The capabilities an adapter declares are the ones its CLI declares.
pub async fn capabilities_are_the_ones_the_cli_declares(subject: &dyn Subject) {
    let declared = subject.adapter(AdapterSettings::default()).capabilities();
    let expected = subject.expected_capabilities();
    assert_eq!(declared, expected);
}

/// A command a session ran is identified by its digest, and the log holds no fragment of it.
pub async fn a_tool_use_never_persists_the_command_it_ran(subject: &dyn Subject) {
    let command = "psql postgres://admin:hunter2@db.internal/prod -c 'select 1'";
    let steps = vec![Step::ToolUse {
        name: "Bash".to_string(),
        target: command.to_string(),
    }];
    let played = run(subject, steps, |_| {}).await;
    let targets: Vec<_> = played
        .events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolUse { target, .. } => Some(target),
            _ => None,
        })
        .collect();
    assert_eq!(targets.len(), 1, "{:?}", played.events);
    assert_eq!(targets[0].display(), None);
    let carried = format!("{:?}", targets[0]);
    for fragment in ["psql", "hunter2", "db.internal", "select"] {
        assert!(!carried.contains(fragment), "`{fragment}` in `{carried}`");
    }
    let digest = sha256_hex(command.as_bytes());
    assert_eq!(*targets[0].digest(), digest);
}

/// The prompt reaches the CLI off its argument list, so no argument carries any of it.
pub async fn the_prompt_never_reaches_argv(subject: &dyn Subject) {
    let played = run(subject, Vec::new(), |req| {
        req.prompt = "the whole brief, with a --flag-looking line".to_string();
    })
    .await;
    let argv = &played.argv;
    let carried = argv.iter().any(|arg| arg.contains("the whole brief"));
    assert!(!carried, "{argv:?}");
}
