//! Identifiers are parsed, never assumed: every newtype checks its rule
//! on construction and on deserialization, so an invalid identifier is
//! unrepresentable past the frontier that read it.

use yunta_core::events::{EventPayload, RunnerResolvedPayload};
use yunta_core::{
    AdapterId, AgentName, ExecutorName, FindingId, InvalidId, Ledger, ModeName, ModelName, NodeId,
    PackManifest, PackName, PackRef, Pid, Publisher, QuestionId, RunId, RunnerName, Seq, SessionId,
    TaskId, Workflow,
};

fn rule_of<T>(result: Result<T, InvalidId>) -> String {
    match result {
        Ok(_) => panic!("expected the value to be refused"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn a_short_name_is_a_letter_followed_by_letters_digits_underscore_or_dash() {
    for accepted in ["plan", "review-alt", "T001", "a_b", "Z"] {
        assert!(accepted.parse::<RunnerName>().is_ok(), "{accepted}");
        assert!(accepted.parse::<ModeName>().is_ok(), "{accepted}");
        assert!(accepted.parse::<TaskId>().is_ok(), "{accepted}");
        assert!(accepted.parse::<QuestionId>().is_ok(), "{accepted}");
        assert!(accepted.parse::<AdapterId>().is_ok(), "{accepted}");
        assert!(accepted.parse::<ExecutorName>().is_ok(), "{accepted}");
    }
    for refused in ["", "1abc", "a b", "a/b", "-x", "a.b", "review@alt", "ñ"] {
        assert!(refused.parse::<RunnerName>().is_err(), "{refused:?}");
        assert!(refused.parse::<ModeName>().is_err(), "{refused:?}");
        assert!(refused.parse::<TaskId>().is_err(), "{refused:?}");
        assert!(refused.parse::<QuestionId>().is_err(), "{refused:?}");
        assert!(refused.parse::<AdapterId>().is_err(), "{refused:?}");
        assert!(refused.parse::<ExecutorName>().is_err(), "{refused:?}");
    }
}

#[test]
fn an_invalid_identifier_names_the_value_the_kind_and_the_rule() {
    let message = rule_of("1abc".parse::<RunnerName>());
    assert_eq!(
        message,
        "`1abc` is not a valid runner name: a letter followed by letters, digits, `_` or `-`"
    );
}

#[test]
fn a_node_id_is_a_short_name_or_a_fan_out_sibling() {
    assert!("review".parse::<NodeId>().is_ok());
    assert!("review@alt".parse::<NodeId>().is_ok());
    for refused in ["review@", "@alt", "review@alt@x", "review@1", "a b@c"] {
        assert!(refused.parse::<NodeId>().is_err(), "{refused:?}");
    }
}

#[test]
fn a_fan_out_sibling_id_carries_its_base_and_runner() {
    let base: NodeId = "review".parse().unwrap();
    let runner: RunnerName = "alt".parse().unwrap();
    let sibling = NodeId::fan_out(&base, &runner);
    assert_eq!(sibling.as_str(), "review@alt");
    assert!(sibling.is_fan_out());
    assert_eq!(sibling.base(), base);
    assert_eq!(sibling.runner(), Some(runner));

    assert!(!base.is_fan_out());
    assert_eq!(base.base(), base);
    assert_eq!(base.runner(), None);
}

#[test]
fn an_authored_node_id_never_uses_the_fan_out_separator() {
    let yaml = "\
name: fan
nodes:
  - id: review@alt
    kind: prompt
    prompt: audit
";
    let error = yunta_core::yaml::parse::<Workflow>(yaml)
        .unwrap_err()
        .to_string();
    assert!(error.contains("review@alt"), "{error}");
    assert!(error.contains("`@`"), "{error}");
    assert!(error.contains("runners:"), "{error}");
}

#[test]
fn a_finding_id_is_any_printable_label() {
    for accepted in [
        "f1",
        "distill-missing-findings.yaml",
        "review@alt-review-0",
        "T001 x",
    ] {
        assert!(accepted.parse::<FindingId>().is_ok(), "{accepted}");
    }
    for refused in ["", "a\nb", "\u{7f}"] {
        assert!(refused.parse::<FindingId>().is_err(), "{refused:?}");
    }
}

#[test]
fn a_model_or_agent_name_is_printable_ascii_without_whitespace() {
    for accepted in ["claude-sonnet-4-6", "gpt-5-codex", "o3", "org/model:latest"] {
        assert!(accepted.parse::<ModelName>().is_ok(), "{accepted}");
        assert!(accepted.parse::<AgentName>().is_ok(), "{accepted}");
    }
    for refused in ["", "claude 3", "tab\tname", "ñandú", "new\nline"] {
        assert!(refused.parse::<ModelName>().is_err(), "{refused:?}");
        assert!(refused.parse::<AgentName>().is_err(), "{refused:?}");
    }
}

#[test]
fn a_run_id_is_one_path_segment() {
    for accepted in [
        "run-20260901-120000-42",
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "run-1-plan",
    ] {
        assert!(accepted.parse::<RunId>().is_ok(), "{accepted}");
    }
    for refused in ["", ".", "..", "a/b", "a\\b", "run 1", "run\n1"] {
        assert!(refused.parse::<RunId>().is_err(), "{refused:?}");
    }
}

#[test]
fn a_session_id_is_opaque_but_never_empty_or_control_characters() {
    assert!("8f0c1e1a-3b2d-4c5e-9f7a-0b1c2d3e4f5a"
        .parse::<SessionId>()
        .is_ok());
    assert!("thread with spaces".parse::<SessionId>().is_ok());
    assert!("".parse::<SessionId>().is_err());
    assert!("a\nb".parse::<SessionId>().is_err());
}

#[test]
fn a_publisher_and_a_pack_name_are_path_segments() {
    for accepted in ["acme", "review-pack", "org.unit"] {
        assert!(accepted.parse::<Publisher>().is_ok(), "{accepted}");
        assert!(accepted.parse::<PackName>().is_ok(), "{accepted}");
    }
    for refused in ["", ".", "..", "acme/review", "a b", "back\\slash"] {
        assert!(refused.parse::<Publisher>().is_err(), "{refused:?}");
        assert!(refused.parse::<PackName>().is_err(), "{refused:?}");
    }
}

#[test]
fn a_pack_reference_is_publisher_slash_name() {
    let reference: PackRef = "acme/review-pack".parse().unwrap();
    assert_eq!(reference.publisher().as_str(), "acme");
    assert_eq!(reference.name().as_str(), "review-pack");
    assert_eq!(reference.to_string(), "acme/review-pack");
    assert_eq!(
        reference,
        PackRef::new("acme".parse().unwrap(), "review-pack".parse().unwrap())
    );
    for refused in ["acme", "acme/", "/review", "a/b/c", "", "a b/c"] {
        assert!(refused.parse::<PackRef>().is_err(), "{refused:?}");
    }
    let message = rule_of("acme".parse::<PackRef>());
    assert!(message.contains("publisher/name"), "{message}");
}

#[test]
fn a_pack_reference_serializes_as_its_text_form() {
    let reference: PackRef = "acme/review-pack".parse().unwrap();
    assert_eq!(
        serde_json::to_string(&reference).unwrap(),
        "\"acme/review-pack\""
    );
    let back: PackRef = serde_json::from_str("\"acme/review-pack\"").unwrap();
    assert_eq!(back, reference);
    assert!(serde_json::from_str::<PackRef>("\"acme\"").is_err());
}

#[test]
fn a_pid_is_never_zero() {
    assert_eq!(Pid::try_from(42_u32).unwrap().to_string(), "42");
    assert_eq!(Pid::try_from(7_i32).unwrap().as_u32(), 7);
    assert!(Pid::try_from(0_u32).is_err());
    assert!(Pid::try_from(0_i32).is_err());
    assert!(Pid::try_from(-1_i32).is_err());
    assert!(serde_json::from_str::<Pid>("0").is_err());
    assert_eq!(serde_json::from_str::<Pid>("42").unwrap().as_u32(), 42);
    assert!(Pid::current().as_u32() > 0);
}

#[test]
fn a_pid_fits_a_signed_32_bit_integer() {
    assert_eq!(Pid::try_from(7_u32).unwrap().as_i32(), 7);
    assert_eq!(Pid::try_from(i32::MAX as u32).unwrap().as_i32(), i32::MAX);
    assert!(Pid::try_from(i32::MAX as u32 + 1).is_err());
    assert!(serde_json::from_str::<Pid>("4294967295").is_err());
}

#[test]
fn deserializing_an_identifier_applies_its_rule_and_names_the_path() {
    let yaml = "\
tasks:
  - id: 1-bad-id
    scope: [src/**]
    criteria:
      - cmd: \"true\"
";
    let error = yunta_core::yaml::parse::<Ledger>(yaml)
        .unwrap_err()
        .to_string();
    assert!(error.contains("tasks[0]"), "{error}");
    assert!(error.contains("line 2"), "{error}");
    assert!(
        error.contains("`1-bad-id` is not a valid task id"),
        "{error}"
    );
    assert!(
        error.contains("a letter followed by letters, digits, `_` or `-`"),
        "{error}"
    );
}

#[test]
fn a_pack_manifest_declares_the_runners_it_requires() {
    let yaml = "\
name: review-pack
publisher: acme
version: 1.0.0
requires:
  runners:
    - { name: reviewer, permissions: read-only }
    - { name: mechanical }
declares:
  permissions: read-only
";
    let pack: PackManifest = yunta_core::yaml::parse(yaml).unwrap();
    assert_eq!(pack.requires.runners.len(), 2);
    assert_eq!(pack.requires.runners[0].name.as_str(), "reviewer");
    assert_eq!(pack.publisher.as_str(), "acme");
    assert_eq!(pack.name.as_str(), "review-pack");

    let former = yaml.replace("runners:", "roles:");
    let error = yunta_core::yaml::parse::<PackManifest>(&former)
        .unwrap_err()
        .to_string();
    assert!(error.contains("roles"), "{error}");
    assert!(error.contains("runners"), "{error}");
}

#[test]
fn a_pack_manifest_refuses_a_publisher_that_is_not_a_segment_at_parse_time() {
    let yaml = "\
name: review-pack
publisher: acme/evil
version: 1.0.0
declares:
  permissions: read-only
";
    let error = yunta_core::yaml::parse::<PackManifest>(yaml)
        .unwrap_err()
        .to_string();
    assert!(error.contains("publisher"), "{error}");
    assert!(error.contains("acme/evil"), "{error}");
}

#[test]
fn runner_resolved_names_its_runner_and_still_reads_logs_written_with_role() {
    let candidate = serde_json::json!({ "adapter": "claude-code", "model": "claude-sonnet-4-6" });
    let former = serde_json::json!({
        "kind": "runner_resolved",
        "role": "planner",
        "chosen": candidate,
        "discarded": []
    });
    let payload: EventPayload = serde_json::from_value(former).unwrap();
    let EventPayload::RunnerResolved(resolved) = payload else {
        panic!("expected runner_resolved");
    };
    assert_eq!(resolved.runner.as_str(), "planner");

    let written = serde_json::to_value(EventPayload::RunnerResolved(RunnerResolvedPayload {
        runner: "planner".parse().unwrap(),
        chosen: serde_json::from_value(candidate).unwrap(),
        discarded: Vec::new(),
    }))
    .unwrap();
    assert_eq!(written["runner"], "planner");
    assert!(written.get("role").is_none());
}

#[test]
fn the_default_mode_is_a_valid_mode_name() {
    assert_eq!(ModeName::default().as_str(), "default");
    assert_eq!(ModeName::default(), "default".parse::<ModeName>().unwrap());
}

#[test]
fn a_static_identifier_is_checked_when_the_program_is_built() {
    static MOCK: AdapterId = AdapterId::from_static("mock");
    assert_eq!(MOCK.as_str(), "mock");
    assert_eq!(MOCK, "mock".parse::<AdapterId>().unwrap());
}

#[test]
fn a_seq_starts_at_one_and_counts_up() {
    assert!(Seq::try_from(0_i64).is_err());
    assert!(Seq::try_from(-3_i64).is_err());
    let first = Seq::try_from(1_i64).unwrap();
    assert_eq!(first, Seq::FIRST);
    assert_eq!(first.next().get(), 2);
    assert_eq!(first.to_string(), "1");
    assert_eq!(serde_json::to_string(&first).unwrap(), "1");
    assert!(serde_json::from_str::<Seq>("0").is_err());
    assert_eq!(serde_json::from_str::<Seq>("7").unwrap().get(), 7);
}
