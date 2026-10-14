//! A task's id: the name a task carries in its tasks document.

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{is_name, string_id, InvalidId, NAME_RULE};

string_id!(
    /// A task's id from the tasks document, unique within one tasks document:
    /// `^[A-Za-z][A-Za-z0-9_-]*$`.
    TaskId, what = "task id", rule = NAME_RULE, check = is_name
);
