//! Events a log carries that this binary does not know.

/// How many events a log carries under one `kind` this binary does not
/// know — what `status`, the receipt and `stats` show so a partially
/// interpreted run is never mistaken for a fully interpreted one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UnknownKindCount {
    pub kind: String,
    pub events: usize,
}
