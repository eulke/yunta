//! How a [`Value`] is read from a document, and read again as the type
//! a caller chose.

use std::fmt;

use serde::de::value::{MapDeserializer, SeqDeserializer};
use serde::de::{self, IntoDeserializer, Visitor};
use serde::{Deserialize, Deserializer};

use super::{Mapping, Number, Value};

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

impl<'de> Deserialize<'de> for Mapping {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::Mapping(mapping) => Ok(mapping),
            Value::Null => Ok(Mapping::new()),
            other => Err(de::Error::invalid_type(other.unexpected(), &"a mapping")),
        }
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any YAML value")
    }

    fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
        Ok(Value::Bool(b))
    }

    fn visit_i64<E>(self, n: i64) -> Result<Value, E> {
        Ok(match u64::try_from(n) {
            Ok(n) => Value::Number(Number::Unsigned(n)),
            Err(_) => Value::Number(Number::Signed(n)),
        })
    }

    fn visit_u64<E>(self, n: u64) -> Result<Value, E> {
        Ok(Value::Number(Number::Unsigned(n)))
    }

    fn visit_f64<E>(self, n: f64) -> Result<Value, E> {
        Ok(Value::Number(Number::Float(n)))
    }

    fn visit_str<E>(self, text: &str) -> Result<Value, E> {
        Ok(Value::String(text.to_string()))
    }

    fn visit_string<E>(self, text: String) -> Result<Value, E> {
        Ok(Value::String(text))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        Value::deserialize(deserializer)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Sequence(items))
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut mapping = Mapping::new();
        while let Some((key, value)) = map.next_entry()? {
            mapping.insert(key, value);
        }
        Ok(Value::Mapping(mapping))
    }
}

impl Value {
    /// How serde names this value in an error about its type.
    fn unexpected(&self) -> de::Unexpected<'_> {
        match self {
            Value::Null => de::Unexpected::Unit,
            Value::Bool(b) => de::Unexpected::Bool(*b),
            Value::Number(Number::Unsigned(n)) => de::Unexpected::Unsigned(*n),
            Value::Number(Number::Signed(n)) => de::Unexpected::Signed(*n),
            Value::Number(Number::Float(n)) => de::Unexpected::Float(*n),
            Value::String(text) => de::Unexpected::Str(text),
            Value::Sequence(_) => de::Unexpected::Seq,
            Value::Mapping(_) => de::Unexpected::Map,
        }
    }
}

/// A value read again as the type a caller chose.
impl<'de> Deserializer<'de> for Value {
    type Error = de::value::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self {
            Value::Null => visitor.visit_unit(),
            Value::Bool(b) => visitor.visit_bool(b),
            Value::Number(Number::Unsigned(n)) => visitor.visit_u64(n),
            Value::Number(Number::Signed(n)) => visitor.visit_i64(n),
            Value::Number(Number::Float(n)) => visitor.visit_f64(n),
            Value::String(text) => visitor.visit_string(text),
            Value::Sequence(items) => visitor.visit_seq(SeqDeserializer::new(items.into_iter())),
            Value::Mapping(mapping) => visitor.visit_map(MapDeserializer::new(mapping.into_iter())),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self {
            Value::Null => visitor.visit_none(),
            other => visitor.visit_some(other),
        }
    }

    /// A key written with nothing after it reads as an empty list where
    /// a list is expected — `effects:` with no effects under it.
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self {
            Value::Null => visitor.visit_seq(SeqDeserializer::new(std::iter::empty::<Value>())),
            other => other.deserialize_any(visitor),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    /// And as an empty mapping where a mapping is expected.
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self {
            Value::Null => {
                visitor.visit_map(MapDeserializer::new(std::iter::empty::<(Value, Value)>()))
            }
            other => other.deserialize_any(visitor),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    /// A unit variant is written as its name; one that carries data, as
    /// a mapping of its name to the data.
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self {
            Value::String(variant) => visitor.visit_enum(variant.into_deserializer()),
            Value::Mapping(mapping) if mapping.len() == 1 => visitor.visit_enum(
                de::value::MapAccessDeserializer::new(MapDeserializer::new(mapping.into_iter())),
            ),
            other => Err(de::Error::invalid_type(
                other.unexpected(),
                &"an enum variant",
            )),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct identifier ignored_any
    }
}

impl<'de> IntoDeserializer<'de, de::value::Error> for Value {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}
