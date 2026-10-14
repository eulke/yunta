//! What a person or an agent can type next, as data. The CLI writes each
//! step as the command it accepts.

use serde::{Deserialize, Serialize};

use crate::ids::{NodeId, OptionId, RunId};

/// What a person or an agent can type next. Data; `advice::spell` writes each one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum NextStep {
    Status {
        run: RunId,
        node: Option<NodeId>,
    },
    Resume {
        run: RunId,
    },
    Cancel {
        run: RunId,
    },
    Close {
        run: RunId,
    },
    Receipt {
        run: RunId,
    },
    Verify {
        run: RunId,
    },
    /// `text`: the option needs a free-text answer.
    ResolveGate {
        run: RunId,
        option: Option<OptionId>,
        text: bool,
    },
    Gc,
    /// Check the workflow that composes a refused child: its check names each rule in words.
    Check {
        workflow: String,
    },
}

/// How a surface writes a [`NextStep`] as the command a person types. The
/// CLI passes its own spelling; a surface that draws a next step takes one
/// rather than spelling it.
pub type Spell = std::sync::Arc<dyn Fn(&NextStep) -> String + Send + Sync>;
