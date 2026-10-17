//! A finding's id: one label per finding within its findings artifact.

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{is_opaque, string_id, InvalidId, OPAQUE_RULE};

string_id!(
    /// A finding's id within a `kind: findings` artifact — one printable
    /// label, unique per author. The engine's own findings compose theirs
    /// from the node or artifact they concern.
    FindingId, what = "finding id", rule = OPAQUE_RULE, check = is_opaque
);
