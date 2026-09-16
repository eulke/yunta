//! A list that is never empty, checked once where it is built.
//!
//! Some facts have no meaning with nothing in them: an escalation with
//! an empty menu refuses every answer, and a node that asked no
//! questions is a node that never waited. Those are invariants a
//! constructor can hold, and a type can carry past it, so no reader has
//! to test for a case the writer could not have produced.

use std::fmt;

/// A `Vec<T>` with at least one element.
///
/// Built through [`NonEmpty::new`], which answers `None` for an empty
/// one, or from a first element and the rest. Reads as a slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonEmpty<T>(Vec<T>);

impl<T> NonEmpty<T> {
    /// The list, if it has anything in it.
    pub fn new(items: Vec<T>) -> Option<Self> {
        (!items.is_empty()).then_some(NonEmpty(items))
    }

    /// The first element — the one this type promises exists.
    pub fn first(&self) -> &T {
        // The constructor refuses an empty vector and nothing else
        // builds one, so index 0 is the element the type stands for.
        #[allow(clippy::indexing_slicing)]
        &self.0[0]
    }

    /// How many elements there are — one or more.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always false. Present so a reader reaching for the usual pair
    /// finds the answer the type already guarantees rather than a
    /// missing method.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The elements, in order.
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Gives the elements up as an ordinary list — for a payload field
    /// whose wire shape is a plain array.
    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T> From<(T, Vec<T>)> for NonEmpty<T> {
    /// A first element and whatever follows it: the shape a caller has
    /// when it knows it has one, so nothing can answer `None`.
    fn from((first, rest): (T, Vec<T>)) -> Self {
        let mut items = Vec::with_capacity(rest.len() + 1);
        items.push(first);
        items.extend(rest);
        NonEmpty(items)
    }
}

impl<T> AsRef<[T]> for NonEmpty<T> {
    fn as_ref(&self) -> &[T] {
        &self.0
    }
}

impl<T> IntoIterator for NonEmpty<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a NonEmpty<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// What a caller gets back when it asked for a list it could not
/// promise: the name of what was empty, so the message says which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Empty(pub &'static str);

impl fmt::Display for Empty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` has nothing in it", self.0)
    }
}

impl std::error::Error for Empty {}
