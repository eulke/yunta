//! The hook's side of the rail: how a hook asks the engine, and the answer
//! it gives when the engine cannot be asked.
//!
//! [`settle`] is the one decision a hook makes. It asks the engine within
//! the moment's window. When no answer comes, it reports the failure once,
//! if the log records that failure, and gives the moment's policy as its
//! answer.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::answer::HookAnswer;
use super::window::{AskBudget, Wait, REPORT_MS};
use super::{RailCall, RailEndpoint, RailEvent};
use crate::SessionId;

/// Why a hook got no answer from the engine. `Unreachable` and `NoEngine`
/// never reach the log: no engine is listening to record them.
#[derive(Debug, Clone, Error)]
pub enum RailFailure {
    #[error("the hook call did not read: {0}")]
    Undecodable(String),
    #[error("this session's environment names no engine to ask")]
    NoEngine,
    #[error("the engine's listener did not take the call: {0}")]
    Unreachable(String),
    #[error("the engine did not answer within the window")]
    TimedOut,
    #[error("the engine answered {0}")]
    Refused(u16),
    #[error("the engine's answer does not read: {0}")]
    Unanswered(String),
}

impl RailFailure {
    /// The cause the log records this failure under, if it records one. An
    /// undecodable call and an unreadable answer are `Decode`, a spent
    /// window is `Timeout`, and the rest are not reported.
    pub fn reportable(&self) -> Option<RailFailureCause> {
        match self {
            RailFailure::Undecodable(_) | RailFailure::Unanswered(_) => {
                Some(RailFailureCause::Decode)
            }
            RailFailure::TimedOut => Some(RailFailureCause::Timeout),
            RailFailure::NoEngine | RailFailure::Unreachable(_) | RailFailure::Refused(_) => None,
        }
    }
}

/// The causes a failure reaches the log under: a closed set, so every
/// surface names a cause the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RailFailureCause {
    /// A call or an answer that does not read.
    Decode,
    /// The engine refused the session's credential.
    Unauthorized,
    /// The engine did not answer within the window.
    Timeout,
    /// A formatter could not be started.
    FormatterFailed,
    /// The listener that serves the session stopped.
    ListenerDied,
    /// A stop's judgement failed, and the stop went on.
    JudgementFailed,
    /// The session asked with no fence to judge its writes by.
    NoFence,
}

impl RailFailureCause {
    /// The word the log holds for this cause, the one serde writes too.
    pub fn as_str(self) -> &'static str {
        match self {
            RailFailureCause::Decode => "decode",
            RailFailureCause::Unauthorized => "unauthorized",
            RailFailureCause::Timeout => "timeout",
            RailFailureCause::FormatterFailed => "formatter_failed",
            RailFailureCause::ListenerDied => "listener_died",
            RailFailureCause::JudgementFailed => "judgement_failed",
            RailFailureCause::NoFence => "no_fence",
        }
    }
}

/// What a hook sends the engine's listener to record one failure. The
/// listener records it under the session that owns the listener, whatever
/// `session_id` the body names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RailReport {
    pub session_id: Option<SessionId>,
    pub event: RailEvent,
    pub cause: RailFailureCause,
}

/// The engine's answer at `A::MOMENT`, or the moment's policy after a
/// failure. The failure is reported once, when an engine is listening to
/// record it.
pub fn settle<A: HookAnswer>(
    endpoint: Result<&RailEndpoint, RailFailure>,
    call: Result<RailCall, RailFailure>,
    wait: Wait,
) -> A {
    let session = call.as_ref().ok().map(|call| call.session_id.clone());
    let asked = match (&endpoint, &call) {
        (Err(failure), _) | (_, Err(failure)) => Err(failure.clone()),
        (Ok(endpoint), Ok(call)) => endpoint.ask::<A>(call, wait),
    };
    asked.unwrap_or_else(|failure| {
        if let (Ok(endpoint), Some(cause)) = (&endpoint, failure.reportable()) {
            let report = RailReport {
                session_id: session,
                event: A::MOMENT,
                cause,
            };
            // best-effort: a report that never arrives leaves the refusal as the only trace
            endpoint.report(&report).ok();
        }
        A::unasked(&failure.to_string())
    })
}

impl RailEndpoint {
    /// Asks the engine about `call` and reads the answer typed for the
    /// moment `call` is about. Each step of the call stays within `wait`;
    /// a judgement has no bound.
    ///
    /// Plain HTTP/1.1 over a blocking socket: a hook is a fresh process per
    /// call, and an async runtime would cost more to start than the call
    /// takes.
    pub fn ask<A: HookAnswer>(&self, call: &RailCall, wait: Wait) -> Result<A, RailFailure> {
        if call.act.moment() != A::MOMENT {
            return Err(RailFailure::Undecodable(format!(
                "a `{}` call cannot be answered as `{}`",
                call.act.moment(),
                A::MOMENT
            )));
        }
        self.answer_call(call, wait).map_err(failure_of)
    }

    /// Reports one failure to the engine's listener, with the session's
    /// token. A report is sent once, never retried, and bounded by its own
    /// window, so it cannot push a hook past its timeout.
    pub fn report(&self, report: &RailReport) -> Result<(), RailFailure> {
        self.send_report(report).map_err(failure_of)
    }

    /// The typed answer to `call`, read over one exchange with the engine.
    fn answer_call<A: HookAnswer>(&self, call: &RailCall, wait: Wait) -> Result<A, RailAskError> {
        let body = serde_json::to_vec(call).map_err(RailAskError::Encode)?;
        let response = self.exchange(A::MOMENT.as_str(), &body, Limit::from(wait))?;
        answer_of(&response)
    }

    /// Sends one report over one exchange, which the report window bounds in total.
    fn send_report(&self, report: &RailReport) -> Result<(), RailAskError> {
        let body = serde_json::to_vec(report).map_err(RailAskError::Encode)?;
        let window = Duration::from_millis(REPORT_MS);
        let limit = Instant::now()
            .checked_add(window)
            .map_or(Limit::Open, Limit::Until);
        let response = self.exchange("report", &body, limit)?;
        body_of(&response).map(|_| ())
    }

    /// One POST to the route `word` names under this endpoint, with `body`,
    /// read to the end of the response. Every step stays within `limit`.
    fn exchange(&self, word: &str, body: &[u8], limit: Limit) -> Result<Vec<u8>, RailAskError> {
        let (authority, path) = split_url(&self.url)?;
        let address = authority
            .to_socket_addrs()
            .map_err(RailAskError::Connect)?
            .next()
            .ok_or_else(|| RailAskError::Url(self.url.clone()))?;
        let mut stream = connect(address, limit)?;
        let head = format!(
            "POST {path}/{word} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.token.expose(),
            body.len()
        );
        let mut wire = Bounded {
            stream: &mut stream,
            limit,
        };
        wire.write_all(head.as_bytes()).map_err(RailAskError::Io)?;
        wire.write_all(body).map_err(RailAskError::Io)?;
        let mut response = Vec::new();
        wire.read_to_end(&mut response).map_err(RailAskError::Io)?;
        Ok(response)
    }
}

/// Why a call to the engine broke off before its answer could be read.
#[derive(Debug)]
enum RailAskError {
    Url(String),
    Connect(io::Error),
    Io(io::Error),
    Encode(serde_json::Error),
    /// The status code of the response, `None` when it has no status line.
    Status(Option<u16>),
    Decode(serde_json::Error),
}

/// The failure a hook reports for a call that broke off or did not read.
fn failure_of(error: RailAskError) -> RailFailure {
    match error {
        RailAskError::Connect(error) | RailAskError::Io(error) if timed_out(&error) => {
            RailFailure::TimedOut
        }
        RailAskError::Connect(error) | RailAskError::Io(error) => {
            RailFailure::Unreachable(error.to_string())
        }
        RailAskError::Url(url) => RailFailure::Unreachable(format!("`{url}` is not an HTTP URL")),
        RailAskError::Encode(error) => RailFailure::Unreachable(error.to_string()),
        RailAskError::Status(Some(code)) if code != 200 => RailFailure::Refused(code),
        RailAskError::Status(_) => {
            RailFailure::Unanswered("the response has no status line".to_string())
        }
        RailAskError::Decode(error) => RailFailure::Unanswered(error.to_string()),
    }
}

/// Whether an I/O error is a deadline that ran out, at connect or at read.
fn timed_out(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

/// The time a call to the engine may still take, bounded by one of three
/// limits.
#[derive(Debug, Clone, Copy)]
enum Limit {
    /// The write window: the answer must land before its budget is spent.
    Window(AskBudget),
    /// A fixed end, for a report.
    Until(Instant),
    /// No end at all, for a judgement.
    Open,
}

impl Limit {
    /// The time the next step may take: `None` without a limit, and a
    /// timeout error once the limit is spent.
    fn bound(self, now: Instant) -> io::Result<Option<Duration>> {
        let left = match self {
            Limit::Window(budget) => budget.remaining(now),
            Limit::Until(end) => end
                .checked_duration_since(now)
                .filter(|left| !left.is_zero()),
            Limit::Open => return Ok(None),
        };
        left.map(Some)
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    }
}

impl From<Wait> for Limit {
    fn from(wait: Wait) -> Self {
        match wait {
            Wait::Write(budget) => Limit::Window(budget),
            Wait::Judgement => Limit::Open,
        }
    }
}

/// A connection whose every read and write is bounded by what is left of
/// its limit, so a slow engine cannot stretch a call past its window.
struct Bounded<'a> {
    stream: &'a mut TcpStream,
    limit: Limit,
}

impl Read for Bounded<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream
            .set_read_timeout(self.limit.bound(Instant::now())?)?;
        self.stream.read(buf)
    }
}

impl Write for Bounded<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream
            .set_write_timeout(self.limit.bound(Instant::now())?)?;
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

/// Opens the connection to `address`, with its connect bounded by `limit`.
fn connect(address: SocketAddr, limit: Limit) -> Result<TcpStream, RailAskError> {
    let connected = match limit.bound(Instant::now()).map_err(RailAskError::Connect)? {
        Some(left) => TcpStream::connect_timeout(&address, left),
        None => TcpStream::connect(address),
    };
    connected.map_err(RailAskError::Connect)
}

/// `http://host:port/path` as its authority and its path.
fn split_url(url: &str) -> Result<(&str, &str), RailAskError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| RailAskError::Url(url.to_string()))?;
    Ok(match rest.find('/') {
        Some(slash) => rest.split_at(slash),
        None => (rest, ""),
    })
}

/// The typed answer a response carries in its body.
fn answer_of<A: DeserializeOwned>(response: &[u8]) -> Result<A, RailAskError> {
    let body = body_of(response)?;
    serde_json::from_str(&body).map_err(RailAskError::Decode)
}

/// The body of a response whose status is 200, or the status it has instead.
fn body_of(response: &[u8]) -> Result<String, RailAskError> {
    let text = String::from_utf8_lossy(response);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or(RailAskError::Status(None))?;
    match status_of(head) {
        Some(200) if chunked(head) => Ok(dechunked(body)),
        Some(200) => Ok(body.to_string()),
        code => Err(RailAskError::Status(code)),
    }
}

/// The status code on a response's first line, when it has one.
fn status_of(head: &str) -> Option<u16> {
    head.lines().next()?.split_whitespace().nth(1)?.parse().ok()
}

/// Whether a response's head says its body is chunked.
fn chunked(head: &str) -> bool {
    head.lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"))
}

/// A chunked body joined back together.
fn dechunked(body: &str) -> String {
    let mut joined = String::new();
    let mut rest = body;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if size == 0 {
            break;
        }
        let (Some(chunk), Some(tail)) = (after.get(..size), after.get(size..)) else {
            break;
        };
        joined.push_str(chunk);
        rest = tail.trim_start_matches("\r\n");
    }
    joined
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::rail::answer::{StopAnswer, WriteAnswer};
    use crate::rail::RailAct;
    use crate::Secret;

    #[test]
    fn an_answer_reads_out_of_a_plain_or_chunked_response() {
        let plain = b"HTTP/1.1 200 OK\r\ncontent-length: 20\r\n\r\n{\"answer\":\"proceed\"}";
        assert_eq!(answer_of::<StopAnswer>(plain).unwrap(), StopAnswer::Proceed);
        let chunked = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n\
                        14\r\n{\"answer\":\"proceed\"}\r\n0\r\n\r\n";
        assert_eq!(
            answer_of::<StopAnswer>(chunked).unwrap(),
            StopAnswer::Proceed
        );
    }

    #[test]
    fn a_refused_status_is_refused_with_its_code() {
        let refused = b"HTTP/1.1 401 Unauthorized\r\n\r\n";
        let failure = failure_of(answer_of::<StopAnswer>(refused).unwrap_err());
        assert!(matches!(failure, RailFailure::Refused(401)), "{failure}");
    }

    #[test]
    fn a_call_is_answered_only_at_its_own_moment() {
        let endpoint = RailEndpoint {
            url: "http://127.0.0.1:9/rail".to_string(),
            token: Secret::new("t".to_string()),
        };
        let call = RailCall {
            session_id: "s-1".parse().unwrap(),
            act: RailAct::AfterWrite { paths: Vec::new() },
            model: None,
        };
        let failure = endpoint
            .ask::<WriteAnswer>(&call, Wait::Judgement)
            .unwrap_err();
        assert!(matches!(failure, RailFailure::Undecodable(_)), "{failure}");
    }

    #[test]
    fn a_failure_to_ask_settles_into_the_moment_policy() {
        let write = RailCall {
            session_id: "s-1".parse().unwrap(),
            act: RailAct::BeforeWrite {
                paths: vec![PathBuf::from("src/a.rs")],
            },
            model: None,
        };
        let wait = Wait::Write(AskBudget::from_process_start(Instant::now()));
        let refused = settle::<WriteAnswer>(Err(RailFailure::NoEngine), Ok(write), wait);
        assert!(matches!(refused, WriteAnswer::Refuse { .. }));
        let stop = RailCall {
            session_id: "s-1".parse().unwrap(),
            act: RailAct::BeforeStop,
            model: None,
        };
        let stopped = settle::<StopAnswer>(Err(RailFailure::TimedOut), Ok(stop), Wait::Judgement);
        assert_eq!(stopped, StopAnswer::Proceed);
    }

    #[test]
    fn only_a_decode_or_timeout_failure_reaches_the_log() {
        assert_eq!(
            RailFailure::TimedOut.reportable(),
            Some(RailFailureCause::Timeout)
        );
        assert_eq!(
            RailFailure::Unanswered(String::new()).reportable(),
            Some(RailFailureCause::Decode)
        );
        assert_eq!(
            RailFailure::Undecodable(String::new()).reportable(),
            Some(RailFailureCause::Decode)
        );
        assert_eq!(RailFailure::Refused(401).reportable(), None);
        assert_eq!(RailFailure::NoEngine.reportable(), None);
        assert_eq!(RailFailure::Unreachable(String::new()).reportable(), None);
    }

    #[test]
    fn a_cause_is_spelled_the_way_serde_writes_it() {
        let causes = [
            RailFailureCause::Decode,
            RailFailureCause::Unauthorized,
            RailFailureCause::Timeout,
            RailFailureCause::FormatterFailed,
            RailFailureCause::ListenerDied,
            RailFailureCause::JudgementFailed,
            RailFailureCause::NoFence,
        ];
        for cause in causes {
            let wire = serde_json::to_value(cause).unwrap();
            assert_eq!(wire.as_str(), Some(cause.as_str()));
        }
    }

    #[test]
    fn a_spent_read_is_a_timeout_and_a_refused_connection_is_unreachable() {
        let spent = io::Error::from(io::ErrorKind::TimedOut);
        assert!(matches!(
            failure_of(RailAskError::Io(spent)),
            RailFailure::TimedOut
        ));
        let refused = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert!(matches!(
            failure_of(RailAskError::Connect(refused)),
            RailFailure::Unreachable(_)
        ));
    }

    #[test]
    fn a_limit_spent_at_its_start_is_a_timeout_and_an_open_one_has_no_bound() {
        let now = Instant::now();
        assert!(matches!(Limit::Open.bound(now), Ok(None)));
        let spent = Limit::Until(now).bound(now).unwrap_err();
        assert_eq!(spent.kind(), io::ErrorKind::TimedOut);
    }
}
