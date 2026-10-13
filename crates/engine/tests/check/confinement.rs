//! The network refusals and the token cap warning, judged against the
//! adapters a workflow's nodes could run on. A bash node opens no session,
//! so no session's network or sandbox refuses it.

use super::*;
use yunta_engine::check_token_cap_warnings;

/// A read-only session is confined with its network closed, so a node that
/// asks for the network is refused before a run.
#[test]
fn a_read_only_node_asking_for_the_network_is_refused() {
    // The node names a runner the config defines, so a session can open and
    // the network is the only thing this node asks wrong.
    let yaml = "name: w\nnodes:\n  - id: r\n    kind: prompt\n    runner: executor\n    prompt: \"go\"\n    permissions: read-only\n    network: true\n";
    let wf = yunta_core::yaml::parse::<Workflow>(yaml).unwrap();
    let errors = check(&wf, &config_with_runner("executor", 1));
    assert!(
        matches!(errors.as_slice(), [CheckError::NetworkOpenUnderReadOnly { node }] if node.as_str() == "r"),
        "a read-only node asking for the network is refused, and nothing else is: {errors:?}"
    );
}

/// A read-only sandbox closes the network of an edit node's session, so a
/// node that asks for the network under it is refused.
#[test]
fn an_edit_node_asking_for_the_network_under_a_read_only_adapter_is_refused() {
    let wf = asking_node(
        "{ id: writer, kind: prompt, runner: planner, prompt: \"edit\", permissions: edit, network: true }",
    );
    let config = config_with_runner("planner", 1);

    let errors = check_against(&wf, &config, &|_| {
        Some(AdapterFacts {
            capabilities: yunta_core::Capabilities::default(),
            edit_sandbox: yunta_core::EditSandbox::ReadOnly,
        })
    });
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::NetworkOpenUnderReadOnly { node } if node.as_str() == "writer"
        )),
        "an edit node asking for network under a read-only sandbox is refused: {errors:?}"
    );
}

/// A bash node runs no agent session, so the sandbox a runner gives its
/// sessions does not reach it: asking for the network there is not refused.
#[test]
fn a_bash_node_asking_for_the_network_under_a_read_only_sandbox_is_not_refused() {
    let wf = asking_node("{ id: fetch, kind: bash, run: \"true\", network: true }");
    let config = config_with_runner("planner", 1);

    let errors = check_against(&wf, &config, &|_| {
        Some(AdapterFacts {
            capabilities: yunta_core::Capabilities::default(),
            edit_sandbox: yunta_core::EditSandbox::ReadOnly,
        })
    });
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::NetworkOpenUnderReadOnly { .. })),
        "a node that runs no session is not held to the sandbox of one: {errors:?}"
    );
}

/// A closed network a project declares is a session's network: a bash node
/// is not refused for it, while a `full` prompt under the same project is.
#[test]
fn a_bash_node_marked_full_is_not_refused_under_a_project_that_closes_the_network() {
    let closed: ConfigLayer =
        yunta_core::yaml::parse("permissions:\n  network:\n    default: false\n").unwrap();

    let bash_node = asking_node("{ id: fetch, kind: bash, run: \"true\", permissions: full }");
    assert!(
        !check(&bash_node, &closed)
            .iter()
            .any(|e| matches!(e, CheckError::ClosedNetworkUnconfined { .. })),
        "a bash node opens no session, so the project's closed network does not refuse it"
    );

    let session_node = asking_node("{ id: ask, kind: prompt, prompt: \"go\", permissions: full }");
    assert!(
        check(&session_node, &closed).iter().any(
            |e| matches!(e, CheckError::ClosedNetworkUnconfined { node } if node.as_str() == "ask")
        ),
        "a full session under the same closed project network is refused"
    );
}

/// A read-only bash node opens no session, so the rule that a read-only
/// session gets no network does not refuse the network it asks for.
#[test]
fn a_bash_node_marked_read_only_asking_for_the_network_is_not_refused() {
    let wf = asking_node(
        "{ id: fetch, kind: bash, run: \"true\", permissions: read-only, network: true }",
    );
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::NetworkOpenUnderReadOnly { .. })),
        "a bash node opens no session, so no read-only session refuses it: {errors:?}"
    );
}

/// An edit session under an adapter whose sandbox is `danger-full-access`
/// runs unconfined, so a node that closes its network there is refused; the
/// same node under a `workspace-write` sandbox is confined and not refused.
#[test]
fn an_edit_node_closing_the_network_under_a_danger_full_access_adapter_is_refused() {
    let wf = asking_node(
        "{ id: writer, kind: prompt, runner: planner, prompt: \"edit\", permissions: edit, network: false }",
    );
    let config = config_with_runner("planner", 1);
    let refused_under = |edit_sandbox: yunta_core::EditSandbox| {
        check_against(&wf, &config, &|_| {
            Some(AdapterFacts {
                capabilities: only(yunta_core::Capability::StopRefusals),
                edit_sandbox,
            })
        })
        .iter()
        .any(|e| {
            matches!(e, CheckError::ClosedNetworkUnconfined { node } if node.as_str() == "writer")
        })
    };
    assert!(
        refused_under(yunta_core::EditSandbox::DangerFullAccess),
        "an edit node closing its network under a danger-full-access sandbox is refused"
    );
    assert!(
        !refused_under(yunta_core::EditSandbox::WorkspaceWrite),
        "the same node under a workspace-write sandbox is not refused"
    );
}

/// The run's token cap is checked when a session ends for an adapter that
/// reports no usage mid-session, so a prompt under the cap warns for such an
/// adapter and not for one that declares `mid_session_token_cap`.
#[test]
fn a_prompt_under_the_token_cap_warns_when_its_adapter_cannot_check_it_mid_session() {
    let wf = asking_node("{ id: audit, kind: prompt, runner: planner, prompt: \"look\" }");
    let capped: ConfigLayer = yunta_core::yaml::parse(
        "runners:\n  planner:\n    - { adapter: mock, model: mock-model }\nlimits:\n  max_tokens_per_run: 1000\n",
    )
    .unwrap();

    let warned = check_token_cap_warnings(&wf, &capped, &|_| facts(Default::default()));
    assert!(
        matches!(warned.as_slice(), [CheckWarning::TokenCapAtSessionEnd { node }] if node.as_str() == "audit"),
        "a candidate with no mid-session cap warns for the prompt under the cap: {warned:?}"
    );

    let checked = check_token_cap_warnings(&wf, &capped, &|_| {
        facts(only(yunta_core::Capability::MidSessionTokenCap))
    });
    assert!(
        checked.is_empty(),
        "a candidate that declares the mid-session cap is not warned: {checked:?}"
    );
}
