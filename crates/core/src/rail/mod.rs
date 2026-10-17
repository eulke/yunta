//! The rail: what a session's CLI asks the engine while it works.
//!
//! A CLI runs a hook at a few moments of a session — before a write,
//! after one, before it stops. Each hook is this binary's `rail`
//! subcommand: the adapter's codec reads what the CLI sent into a
//! [`RailCall`], the call goes to the engine over the session's own
//! listener, and the codec writes the engine's answer back in the shape
//! that CLI reads. Each moment has its own answer type (see [`answer`]),
//! so a write's answer cannot be read as a stop's. The hook decides
//! nothing: the engine judges, and records what it judged, so every
//! adapter's session is ruled and logged the same way.
//!
//! [`hook::settle`] turns a hook's call into its answer. When the engine
//! cannot be asked, the moment's own policy answers: a write is refused,
//! and anything else goes on, because the attempt's close still judges the
//! work. A failure the engine can record reaches its log. The windows that
//! bound each call are in [`window`], and the timing a session keeps is in
//! [`timing`].

pub mod answer;
pub mod hook;
pub mod message;
pub mod timing;
pub mod window;

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{AdapterId, ModelName, Secret, SessionId};

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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
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
    /// The model the CLI runs, when its hook input names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
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
/// session's own listener, and the per-session token that scopes every
/// call to that session. The token is secret material and never reaches
/// the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailEndpoint {
    pub url: String,
    pub token: Secret<String>,
}

impl RailEndpoint {
    /// Puts the variables a session's CLI hands its hooks into `env`: the
    /// address is not a secret, the token is.
    pub fn hand_to(&self, env: &mut crate::port::session::SessionEnv) {
        env.plain.insert(URL_VAR.to_string(), self.url.clone());
        env.secret.insert(TOKEN_VAR.to_string(), self.token.clone());
    }
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
            model: None,
        };
        let wire = serde_json::to_value(&call).unwrap();
        assert_eq!(wire["act"]["event"], "before-write");
        assert_eq!(serde_json::from_value::<RailCall>(wire).unwrap(), call);
    }

    #[test]
    fn a_call_without_a_model_reads_as_none() {
        let wire = serde_json::json!({ "session_id": "s-1", "act": { "event": "before-stop" } });
        let call: RailCall = serde_json::from_value(wire).unwrap();
        assert_eq!(call.model, None);
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
