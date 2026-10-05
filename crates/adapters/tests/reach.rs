//! A probe of a session's service reaches the endpoint the session's CLI
//! would: the proxy or base URL the shell had, never one the adapter read
//! from its own process.

use std::collections::BTreeMap;
use std::net::TcpListener;

use yunta_adapters::ClaudeCodeAdapter;
use yunta_core::port::Adapter;
use yunta_core::AdapterSettings;

fn shell(name: &str, port: u16) -> BTreeMap<String, String> {
    BTreeMap::from([(name.to_string(), format!("http://127.0.0.1:{port}"))])
}

#[tokio::test]
async fn a_probe_reaches_the_proxy_the_shell_had() {
    let listening = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listening.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let req = yunta_testkit_core::adapter::request(dir.path().to_path_buf());
    let adapter = ClaudeCodeAdapter::new(&AdapterSettings::default());

    let answered = adapter.reachable(&req, &shell("HTTPS_PROXY", port)).await;

    assert_eq!(answered, Some(true));
}

#[tokio::test]
async fn a_probe_says_so_when_the_shells_base_url_does_not_answer() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let dir = tempfile::tempdir().unwrap();
    let req = yunta_testkit_core::adapter::request(dir.path().to_path_buf());
    let adapter = ClaudeCodeAdapter::new(&AdapterSettings::default());

    let answered = adapter
        .reachable(&req, &shell("ANTHROPIC_BASE_URL", port))
        .await;

    assert_eq!(answered, Some(false));
}
