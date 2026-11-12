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
