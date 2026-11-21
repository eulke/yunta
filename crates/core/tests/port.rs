//! The port, as a caller meets it.

use yunta_core::port::RunToolsEndpoint;
use yunta_core::Secret;
use yunta_testkit_core::adapter::request;

/// A session request carries three secrets — the environment a child is
/// given, the bearer token of its per-run endpoint and the same token its
/// hooks reach the engine with — and its `Debug`
/// is the likeliest place for either to escape: a trace line, a panic
/// message, a diagnostic a run writes. The names stay visible, because
/// a reader needs to know which variable was set; the values never
/// print.
#[test]
fn debug_of_a_session_request_never_prints_a_secret_it_carries() {
    let mut req = request(std::path::PathBuf::from("/tmp"));
    req.env
        .secret
        .insert("API_TOKEN".to_string(), Secret::new("hunter2".to_string()));
    req.run_tools_endpoint = Some(RunToolsEndpoint {
        url: "http://127.0.0.1:1/mcp".to_string(),
        token: "bearer-secret".to_string().into(),
    });
    req.rail = Some(yunta_core::rail::RailEndpoint {
        url: "http://127.0.0.1:1/rail".to_string(),
        token: "bearer-secret".to_string().into(),
    });

    let debug = format!("{req:?}");
    assert!(
        debug.contains("API_TOKEN"),
        "the name stays visible: {debug}"
    );
    assert!(
        !debug.contains("hunter2"),
        "the value never prints: {debug}"
    );
    assert!(
        !debug.contains("bearer-secret"),
        "the token never prints: {debug}"
    );
    assert_eq!(
        debug.matches("[redacted]").count(),
        3,
        "the env value and both tokens are redacted: {debug}"
    );
}

/// What a session says may repeat any secret it holds: a declared one its
/// environment binds, or the token its run tools and its hooks reach the
/// engine with. The session's redactor takes every one of them out, so
/// words handed to a reader never carry one.
#[test]
fn a_sessions_redactor_takes_out_every_secret_it_holds() {
    let mut req = request(std::path::PathBuf::from("/tmp"));
    req.env.secret.insert(
        "API_TOKEN".to_string(),
        Secret::new("declared-value-1".to_string()),
    );
    req.run_tools_endpoint = Some(RunToolsEndpoint {
        url: "http://127.0.0.1:1/mcp".to_string(),
        token: "tools-token-2".to_string().into(),
    });
    req.rail = Some(yunta_core::rail::RailEndpoint {
        url: "http://127.0.0.1:1/rail".to_string(),
        token: "rail-token-3".to_string().into(),
    });

    let said = "used declared-value-1, then tools-token-2 and rail-token-3";
    assert_eq!(
        req.redactor().text(said),
        "used [redacted], then [redacted] and [redacted]"
    );
}

/// A pull request's handle as a gate's publication keeps it in the log reads back as the pull
/// request it names, and a handle that names none is refused rather than read as one: what
/// a wake that polls the gate, and a screen that reads it, both rest on.
#[test]
fn a_pull_requests_handle_reads_back_and_one_that_names_none_is_refused() {
    let handle = r#"{"url":"https://github.com/acme/web/pull/7","number":7}"#;
    let read = yunta_core::port::PullRequestRef::of_handle(handle).expect("a handle the log keeps");
    assert_eq!(read.url, "https://github.com/acme/web/pull/7");
    assert_eq!(read.number, 7);
    for broken in ["", "#7", r#"{"url":"https://github.com/acme/web/pull/7"}"#] {
        assert!(
            yunta_core::port::PullRequestRef::of_handle(broken).is_err(),
            "`{broken}` names no pull request"
        );
    }
}
