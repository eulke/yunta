//! A YAML value read before its type is known.
//!
//! A custom `Deserialize` that has to look at a document before choosing
//! what to read it as — a node's `kind`, a fixture's `sessions` — reads
//! it into a [`Value`] first and reads that again as the type it chose.
//! A persisted document migrates a field it retired the same way. The
//! mapping keeps the order its keys were written in, so a document
//! written back out reads in the order a person wrote it.

use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

mod read;

/// Any YAML value.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Sequence(Vec<Value>),
    Mapping(Mapping),
}

/// A YAML number, in the widest type that holds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
}

/// A YAML mapping, its entries in the order they were written.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mapping(Vec<(Value, Value)>);

impl Mapping {
    pub fn new() -> Self {
        Mapping::default()
    }

    /// The value under the key `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.0
            .iter_mut()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// `value` under `key`: in the place of an entry already there, last
    /// otherwise.
    pub fn insert(&mut self, key: Value, value: Value) {
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some((_, held)) => *held = value,
            None => self.0.push((key, value)),
        }
    }

    /// Takes the value under `key` out of the mapping.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let at = self.0.iter().position(|(k, _)| k.as_str() == Some(key))?;
        Some(self.0.remove(at).1)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.0.iter_mut().map(|(_, v)| v)
    }

    pub fn keys(&self) -> impl Iterator<Item = &Value> {
        self.0.iter().map(|(k, _)| k)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Value, &Value)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::ops::Index<&str> for Mapping {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }
}

impl IntoIterator for Mapping {
    type Item = (Value, Value);
    type IntoIter = std::vec::IntoIter<(Value, Value)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<(Value, Value)> for Mapping {
    fn from_iter<I: IntoIterator<Item = (Value, Value)>>(entries: I) -> Self {
        let mut mapping = Mapping::new();
        for (key, value) in entries {
            mapping.insert(key, value);
        }
        mapping
    }
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Number(Number::Unsigned(n)) => Some(*n),
            Value::Number(Number::Signed(n)) => u64::try_from(*n).ok(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_sequence(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Sequence(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_mapping(&self) -> Option<&Mapping> {
        match self {
            Value::Mapping(mapping) => Some(mapping),
            _ => None,
        }
    }

    pub fn as_mapping_mut(&mut self) -> Option<&mut Mapping> {
        match self {
            Value::Mapping(mapping) => Some(mapping),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// The value under `key`, when this is a mapping that has one.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_mapping().and_then(|mapping| mapping.get(key))
    }
}

/// The null a missing key or an index past the end reads as, so a test
/// can walk a document without unwrapping every step.
static NULL: Value = Value::Null;

impl std::ops::Index<&str> for Value {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }
}

impl std::ops::Index<usize> for Value {
    type Output = Value;

    fn index(&self, at: usize) -> &Value {
        self.as_sequence()
            .and_then(|items| items.get(at))
            .unwrap_or(&NULL)
    }
}

impl PartialEq<str> for Value {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}

impl PartialEq<&str> for Value {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Value::String(text.to_string())
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Value::String(text)
    }
}

impl From<u32> for Value {
    fn from(n: u32) -> Self {
        Value::Number(Number::Unsigned(n.into()))
    }
}

impl From<u64> for Value {
    fn from(n: u64) -> Self {
        Value::Number(Number::Unsigned(n))
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Number(Number::Unsigned(n)) => serializer.serialize_u64(*n),
            Value::Number(Number::Signed(n)) => serializer.serialize_i64(*n),
            Value::Number(Number::Float(n)) => serializer.serialize_f64(*n),
            Value::String(text) => serializer.serialize_str(text),
            Value::Sequence(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Value::Mapping(mapping) => {
                let mut map = serializer.serialize_map(Some(mapping.len()))?;
                for (key, value) in mapping.iter() {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[test]
    fn a_mapping_keeps_the_order_its_keys_were_written_in() {
        let value: Value = crate::yaml::parse("zeta: 1\nalpha: two\nmid: [3]\n").unwrap();
        let keys: Vec<&str> = value
            .as_mapping()
            .unwrap()
            .iter()
            .filter_map(|(key, _)| key.as_str())
            .collect();
        assert_eq!(keys, ["zeta", "alpha", "mid"]);
        assert_eq!(
            crate::yaml::to_string(&value).unwrap(),
            "zeta: 1\nalpha: two\nmid:\n- 3\n"
        );
    }

    #[test]
    fn a_value_reads_again_as_the_type_a_caller_chose() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Shape {
            Unit,
            Data { side: u32 },
        }
        #[derive(Deserialize, PartialEq, Debug)]
        struct Held {
            name: String,
            count: Option<u64>,
            shapes: Vec<Shape>,
        }
        let value: Value =
            crate::yaml::parse("name: n\nshapes: [Unit, {Data: {side: 3}}]\n").unwrap();
        let held: Held = crate::yaml::from_value(value).unwrap();
        assert_eq!(
            held,
            Held {
                name: "n".to_string(),
                count: None,
                shapes: vec![Shape::Unit, Shape::Data { side: 3 }],
            }
        );
    }
}
