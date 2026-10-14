//! The rail's calls that do not read, or that name another session: each is
//! answered by the listener it reaches, and recorded under the session that
//! listener serves.

use super::*;

/// One raw request to a rail route, as a hook's CLI sends it: the body goes
/// under `endpoint`'s token, and the whole response comes back.
fn post_rail(endpoint: &RailEndpoint, path: &str, body: &str) -> String {
    let authority = endpoint
        .url
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap();
    let mut stream = std::net::TcpStream::connect(authority).unwrap();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        endpoint.token.expose(),
        body.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_that_does_not_read_or_bears_no_token_is_recorded_once_per_cause() {
    // `post_rail` blocks its thread, so the listener needs another one to answer on.
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("solo", None).await;
    let endpoint = session.rail_endpoint();
    let stranger = RailEndpoint {
        token: Secret::new("not-the-token".to_string()),
        ..endpoint.clone()
    };
    for _ in 0..2 {
        let unreadable = post_rail(&endpoint, "/rail/before-write", "{}");
        assert!(unreadable.starts_with("HTTP/1.1 400"), "{unreadable}");
        let unsigned = post_rail(&stranger, "/rail/before-write", "{}");
        assert!(unsigned.starts_with("HTTP/1.1 401"), "{unsigned}");
    }
    let recorded = host
        .events()
        .iter()
        .filter(|event| {
            event
                .payload()
                .is_some_and(|p| p.kind_name() == "rail_failed")
        })
        .count();
    assert_eq!(recorded, 2, "one 400 and one 401, each once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_is_judged_and_recorded_under_the_session_that_owns_its_listener() {
    // `post_rail` blocks its thread, so the listener needs another one to answer on.
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let session = host.session("solo", None).await;
    let checkout = host.attempt_dir();
    let confinement = Confinement::of(ConfinementInput {
        profile: PermissionProfile::Edit,
        edit: EditSandbox::default(),
        scoped: true,
        network: true,
        checkout: &checkout,
        artifact_dir: None,
        shared_dirs: &[],
        protected: Arc::new(Protected::default()),
    })
    .expect("a session over its attempt directory is buildable");
    let fence = Fence {
        allowed: Some(vec![ScopeGlob::from("b.txt")]),
        advice: Advice::ReportFinding,
    };
    session.rails().fence(fence, confinement);
    session.rails().own("listener-session".parse().unwrap());
    let body = json!({
        "session_id": "someone-else",
        "act": { "event": "before-write", "paths": ["a.txt"] },
    });
    let endpoint = session.rail_endpoint();
    let answer = post_rail(&endpoint, "/rail/before-write", &body.to_string());
    assert!(answer.contains("write refused"), "{answer}");
    let refused: Vec<String> = host
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::WriteRefused(refused))) => {
                Some(refused.session_id.as_str().to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(refused, vec!["listener-session".to_string()]);
}
