//! The port, as a caller meets it.

use yunta_core::port::RunToolsEndpoint;
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
        .insert("API_TOKEN".to_string(), "hunter2".to_string().into());
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
