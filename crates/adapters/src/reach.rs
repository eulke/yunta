//! Whether the service a session talks to answers from this machine: the
//! name it resolves to, and a connection it accepts — the two things a
//! session cut off from its service lost.
//!
//! The endpoint is the one the session's own environment points its CLI
//! at: a proxy when one is set, the base URL the CLI is configured with,
//! or the service's own host.

use std::collections::BTreeMap;
use std::time::Duration;

use yunta_core::port::SessionRequest;

/// The proxies every CLI here honors, in the order they are looked at.
pub(crate) const PROXY_VARS: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"];

/// Whether the endpoint `req`'s session would reach answers — `None`
/// when its environment names one this cannot read. `shell` is what the
/// shell had for the variables the session's CLI inherits; `base_var` is
/// the variable that moves the CLI's service; `host` is the service's own.
pub(crate) async fn answers(
    req: &SessionRequest,
    shell: &BTreeMap<String, String>,
    base_var: &str,
    host: &str,
) -> Option<bool> {
    let proxy = PROXY_VARS
        .into_iter()
        .find_map(|name| var(req, shell, name));
    let (host, port) = match proxy.or_else(|| var(req, shell, base_var)) {
        Some(url) => endpoint(&url)?,
        None => (host.to_string(), 443),
    };
    let connect = tokio::net::TcpStream::connect((host.as_str(), port));
    // Long enough for a resolver and a handshake on a slow network,
    // short next to the wait it is one probe of.
    let answered = tokio::time::timeout(Duration::from_secs(5), connect).await;
    Some(matches!(answered, Ok(Ok(_))))
}

/// `name` as the session's environment sets it: what the run hands the
/// session in either of its maps, then what it inherits from the shell.
pub(crate) fn var(
    req: &SessionRequest,
    shell: &BTreeMap<String, String>,
    name: &str,
) -> Option<String> {
    req.env
        .plain
        .get(name)
        .cloned()
        .or_else(|| req.env.secret.get(name).map(|value| value.expose().clone()))
        .or_else(|| shell.get(name).cloned())
        .filter(|value| !value.trim().is_empty())
}

/// The host and port `url` names.
fn endpoint(url: &str) -> Option<(String, u16)> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_string();
    Some((host, parsed.port_or_known_default()?))
}

#[cfg(test)]
mod tests {
    use super::{endpoint, PROXY_VARS};
    use yunta_core::port::Adapter;

    /// A proxy the probe honors is one the shell captures for every
    /// adapter, or the probe would look past the proxy its CLI goes through.
    #[test]
    fn every_adapter_names_the_proxies_its_probe_honors() {
        let settings = yunta_core::AdapterSettings::default();
        let adapters: [Box<dyn Adapter>; 2] = [
            Box::new(crate::ClaudeCodeAdapter::new(&settings)),
            Box::new(crate::CodexAdapter::new(&settings)),
        ];
        for adapter in adapters {
            for proxy in PROXY_VARS {
                assert!(
                    adapter.service_vars().contains(&proxy),
                    "{} misses {proxy}",
                    adapter.id()
                );
            }
        }
    }

    #[test]
    fn an_endpoint_is_its_host_and_its_port_or_the_schemes() {
        assert_eq!(
            endpoint("https://gateway.example.com/anthropic"),
            Some(("gateway.example.com".to_string(), 443))
        );
        assert_eq!(
            endpoint("http://127.0.0.1:8080"),
            Some(("127.0.0.1".to_string(), 8080))
        );
        assert_eq!(endpoint("not a url"), None);
    }
}
