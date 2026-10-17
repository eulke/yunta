//! The variants of a tagged union a committed JSON schema declares, and the
//! fields each one carries.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::{admits_null, json_schema};

/// Every variant of an internally tagged enum in a committed JSON
/// schema: the tag each one carries, the fields of the payload it
/// refers to, and whether each field is an `Option`.
///
/// A field that admits `null` is one the value may leave out;
/// everything else it always carries, a `default` saying only what an
/// older document reads as. A payload that is a choice of shapes
/// carries the fields of every branch, flat, which is how it is
/// written.
pub fn tagged_variants(schema: &Path, union: &str) -> BTreeMap<String, BTreeMap<String, bool>> {
    let parsed = json_schema(schema);
    let mut variants = BTreeMap::new();
    for variant in parsed["$defs"][union]["oneOf"]
        .as_array()
        .unwrap_or_else(|| panic!("`{union}` is a tagged union in `{}`", schema.display()))
    {
        let tag = variant["properties"]["kind"]["const"]
            .as_str()
            .expect("a variant carries its tag");
        let named = variant["$ref"]
            .as_str()
            .and_then(|reference| reference.rsplit('/').next())
            .expect("a variant refers to its payload");
        let payload = &parsed["$defs"][named];
        let mut fields = BTreeMap::new();
        let mut take = |object: &Value| {
            for (field, shape) in object["properties"].as_object().into_iter().flatten() {
                fields.insert(field.clone(), admits_null(shape));
            }
        };
        take(payload);
        for branch in payload["anyOf"].as_array().into_iter().flatten() {
            take(branch);
            // A flattened choice of shapes keeps its fields in each `oneOf` alternative.
            for alternative in branch["oneOf"].as_array().into_iter().flatten() {
                take(alternative);
            }
        }
        variants.insert(tag.to_string(), fields);
    }
    variants
}
