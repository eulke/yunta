//! The rail: what a session's CLI asks the engine while it works.
//!
//! A CLI runs a hook at a few moments of a session — before a write,
//! after one, before it stops. Each hook is this binary's `rail`
//! subcommand: the adapter's codec reads what the CLI sent into a
//! [`RailCall`], the call goes to the engine over the session's own
//! listener, and the codec writes the engine's [`RailAnswer`] back in the
//! shape that CLI reads. The hook decides nothing: the engine judges, and
//! records what it judged, so every adapter's session is ruled and logged
//! the same way.
//!
//! A hook that cannot reach the engine answers [`unasked`]: a write is
//! refused, anything else goes on — the attempt's close still judges the
//! work.

use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{AdapterId, Secret, SessionId};

/// The hidden subcommand a CLI's hook runs. Named here so the CLI that
/// registers it and the adapters that embed it cannot disagree.
pub const SUBCOMMAND: &str = "rail";

/// The variable a hook finds the engine at.
pub const URL_VAR: &str = "YUNTA_RAIL_URL";

/// The variable a hook finds its session's credential in.
pub const TOKEN_VAR: &str = "YUNTA_RAIL_TOKEN";

/// Where the session's listener serves the rail, beside its tools.
pub const ROUTE: &str = "/rail";

/// When in a session a CLI asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RailEvent {
    /// A file tool is about to write.
    BeforeWrite,
    /// A file tool wrote.
    AfterWrite,
    /// The session is about to end its turn, and with it its work.
    BeforeStop,
}

impl RailEvent {
    /// Every event, in the order a session meets them.
    pub const ALL: [RailEvent; 3] = [
        RailEvent::BeforeWrite,
        RailEvent::AfterWrite,
        RailEvent::BeforeStop,
    ];

    /// The word the subcommand and the route take.
    pub fn as_str(self) -> &'static str {
        match self {
            RailEvent::BeforeWrite => "before-write",
            RailEvent::AfterWrite => "after-write",
            RailEvent::BeforeStop => "before-stop",
        }
    }
}

impl fmt::Display for RailEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A word no event is called by.
#[derive(Debug, Error)]
#[error("`{0}` is not a rail event")]
pub struct UnknownRailEvent(String);

impl FromStr for RailEvent {
    type Err = UnknownRailEvent;

    fn from_str(word: &str) -> Result<Self, Self::Err> {
        RailEvent::ALL
            .into_iter()
            .find(|event| event.as_str() == word)
            .ok_or_else(|| UnknownRailEvent(word.to_string()))
    }
}

/// What a CLI asked, read out of its hook's input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RailCall {
    /// The session asking, as its CLI names it.
    pub session_id: SessionId,
    pub act: RailAct,
}

/// What the session is doing when it asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum RailAct {
    /// Every path one call is about to write, as the CLI named them:
    /// absolute, or relative to the session's working directory.
    BeforeWrite { paths: Vec<PathBuf> },
    /// Every path one call wrote, named as for [`RailAct::BeforeWrite`].
    AfterWrite { paths: Vec<PathBuf> },
    /// The session would end here.
    BeforeStop,
}

impl RailAct {
    /// The event this act answers to.
    pub fn moment(&self) -> RailEvent {
        match self {
            RailAct::BeforeWrite { .. } => RailEvent::BeforeWrite,
            RailAct::AfterWrite { .. } => RailEvent::AfterWrite,
            RailAct::BeforeStop => RailEvent::BeforeStop,
        }
    }
}

/// What the engine answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum RailAnswer {
    /// Go on.
    Proceed,
    /// Do not; `reason` is what the model reads.
    Refuse { reason: String },
    /// Go on, and know this: `context` is what the model reads.
    Tell { context: String },
}

/// What a hook answers when it could not ask: a write is refused, because
/// nobody knows what the session may write; anything else goes on, because
/// the close still judges the work.
pub fn unasked(event: RailEvent, why: &str) -> RailAnswer {
    match event {
        RailEvent::BeforeWrite => {
            let reason = format!("yunta: write refused: the engine could not be asked ({why}).");
            RailAnswer::Refuse { reason }
        }
        RailEvent::AfterWrite | RailEvent::BeforeStop => RailAnswer::Proceed,
    }
}

/// The command a CLI's hook runs: this binary, its `rail` subcommand, the
/// adapter whose codec reads the call, and the event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailCommand {
    bin: PathBuf,
}

impl RailCommand {
    /// The binary a CLI will run. The shell resolves this once, at
    /// startup, from its own executable.
    pub fn new(bin: PathBuf) -> Self {
        RailCommand { bin }
    }

    /// The command line a CLI's config embeds.
    pub fn argv(&self, adapter: &AdapterId, event: RailEvent) -> Vec<String> {
        vec![
            self.bin.display().to_string(),
            SUBCOMMAND.to_string(),
            adapter.to_string(),
            event.as_str().to_string(),
        ]
    }
}

/// Where a session's hooks reach the engine: the rail route of the
/// session's own listener, and the single-use token that scopes every
/// call to that session. The token is secret material and never reaches
/// the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailEndpoint {
    pub url: String,
    pub token: Secret<String>,
}

impl RailEndpoint {
    /// The variables a session's CLI hands its hooks.
    pub fn env(&self) -> [(&'static str, Secret<String>); 2] {
        [
            (URL_VAR, Secret::new(self.url.clone())),
            (TOKEN_VAR, self.token.clone()),
        ]
    }

    /// Asks the engine about `call`, giving up after `deadline` — or
    /// waiting for as long as the engine takes, which the CLI that runs
    /// the hook bounds with the hook's own timeout.
    ///
    /// Plain HTTP/1.1 over a blocking socket: a hook is a fresh process
    /// per call, and an async runtime would cost more to start than the
    /// call takes.
    pub fn ask(
        &self,
        call: &RailCall,
        deadline: Option<Duration>,
    ) -> Result<RailAnswer, RailAskError> {
        let (authority, path) = split_url(&self.url)?;
        let path = format!("{path}/{}", call.act.moment());
        let body = serde_json::to_vec(call).map_err(RailAskError::Encode)?;
        let address = authority
            .to_socket_addrs()
            .map_err(RailAskError::Connect)?
            .next()
            .ok_or_else(|| RailAskError::Url(self.url.clone()))?;
        let mut stream = match deadline {
            Some(deadline) => TcpStream::connect_timeout(&address, deadline),
            None => TcpStream::connect(address),
        }
        .map_err(RailAskError::Connect)?;
        stream
            .set_read_timeout(deadline)
            .and_then(|()| stream.set_write_timeout(deadline))
            .map_err(RailAskError::Connect)?;
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.token.expose(),
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|()| stream.write_all(&body))
            .map_err(RailAskError::Io)?;
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(RailAskError::Io)?;
        answer_of(&response)
    }
}

/// Why a hook could not get the engine's answer.
#[derive(Debug, Error)]
pub enum RailAskError {
    #[error("`{0}` is not a loopback HTTP URL")]
    Url(String),
    #[error("the engine's listener did not take the call")]
    Connect(#[source] std::io::Error),
    #[error("the call to the engine broke off")]
    Io(#[source] std::io::Error),
    #[error("the call could not be written")]
    Encode(#[source] serde_json::Error),
    #[error("the engine answered {0}")]
    Status(String),
    #[error("the engine's answer does not read")]
    Decode(#[source] serde_json::Error),
}

/// `http://host:port/path` as its authority and its path.
fn split_url(url: &str) -> Result<(&str, &str), RailAskError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| RailAskError::Url(url.to_string()))?;
    Ok(match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, ""),
    })
}

/// The answer an HTTP response carries, or why it carries none.
fn answer_of(response: &[u8]) -> Result<RailAnswer, RailAskError> {
    let text = String::from_utf8_lossy(response);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| RailAskError::Status("nothing".to_string()))?;
    let status = head.lines().next().unwrap_or_default();
    if !status
        .split_whitespace()
        .nth(1)
        .is_some_and(|code| code == "200")
    {
        return Err(RailAskError::Status(status.to_string()));
    }
    let body = if head
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"))
    {
        dechunked(body)
    } else {
        body.to_string()
    };
    serde_json::from_str(&body).map_err(RailAskError::Decode)
}

/// A chunked body joined back together.
fn dechunked(body: &str) -> String {
    let mut joined = String::new();
    let mut rest = body;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if size == 0 || after.len() < size {
            break;
        }
        joined.push_str(&after[..size]);
        rest = after[size..].trim_start_matches("\r\n");
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_reads_back_from_its_word() {
        for event in RailEvent::ALL {
            assert_eq!(event.as_str().parse::<RailEvent>().unwrap(), event);
        }
        assert!("before-everything".parse::<RailEvent>().is_err());
    }

    #[test]
    fn a_call_names_its_event_on_the_wire() {
        let call = RailCall {
            session_id: "s-1".parse::<SessionId>().unwrap(),
            act: RailAct::BeforeWrite {
                paths: vec![PathBuf::from("src/a.rs")],
            },
        };
        let wire = serde_json::to_value(&call).unwrap();
        assert_eq!(wire["act"]["event"], "before-write");
        assert_eq!(serde_json::from_value::<RailCall>(wire).unwrap(), call);
    }

    #[test]
    fn a_hook_that_could_not_ask_refuses_a_write() {
        assert!(matches!(
            unasked(RailEvent::BeforeWrite, "down"),
            RailAnswer::Refuse { .. }
        ));
    }

    #[test]
    fn an_answer_reads_out_of_a_plain_or_chunked_response() {
        let plain = b"HTTP/1.1 200 OK\r\ncontent-length: 20\r\n\r\n{\"answer\":\"proceed\"}";
        assert_eq!(answer_of(plain).unwrap(), RailAnswer::Proceed);
        let chunked = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n14\r\n{\"answer\":\"proceed\"}\r\n0\r\n\r\n";
        assert_eq!(answer_of(chunked).unwrap(), RailAnswer::Proceed);
        let refused = b"HTTP/1.1 401 Unauthorized\r\n\r\n";
        assert!(matches!(answer_of(refused), Err(RailAskError::Status(_))));
    }

    #[test]
    fn a_command_names_its_adapter_and_its_event() {
        let command = RailCommand::new(PathBuf::from("/bin/yunta"));
        assert_eq!(
            command.argv(
                &"claude-code".parse::<AdapterId>().unwrap(),
                RailEvent::BeforeWrite
            ),
            ["/bin/yunta", "rail", "claude-code", "before-write"]
        );
    }
}
