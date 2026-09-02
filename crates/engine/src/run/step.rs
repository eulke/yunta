//! A step's result: the value it produced, or the early end it reached.

use super::gate_exec::GateStep;
use super::node_exec::NodeEnd;

/// The outcome of a step that either produces a value or ends early. Its
/// `Value`/`Ended` split is what a caller reads in one level where a
/// `Result` nested inside another needed two: the outer `Result` still
/// carries a real `RunError`, while `Ended` carries the node's (or gate's)
/// own graceful early end, never an error. `E` defaults to [`NodeEnd`], the
/// end of a node that finished before producing the value; a gate step ends
/// on a [`GateStep`] instead.
pub(super) enum Step<T, E = NodeEnd> {
    Value(T),
    Ended(E),
}

/// A gate render that either produced its text or ended the gate's turn.
pub(super) type GateRender = Step<String, GateStep>;
