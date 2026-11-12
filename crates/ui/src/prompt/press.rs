/// What a stroke asks of a prompt.
#[derive(Debug, PartialEq, Eq)]
pub enum Press<T> {
    /// It stays, drawn again.
    Stay,
    /// It stays, on another question: what was typed for the last one is no answer
    /// to this one.
    Moved,
    /// It is answered.
    Answer(T),
    /// The person leaves it without an answer.
    Decline,
    /// The person stops the run.
    Interrupt,
}
