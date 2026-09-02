//! `pack.yaml`: the manifest of a distributable,
//! versioned bundle of workflows/skills/knowledge/docs (and, optionally,
//! executors). A pack never extends the engine — it only ships material
//! the engine already knows how to run, the same relationship a Helm
//! chart or a Terraform module has to its runtime.
//!
//! Normative split this type exists to enforce: a pack **requires**
//! runners by name (with the permissions each needs) and **declares** a
//! permissions ceiling it promises never to exceed — never a binding, a
//! concrete model or a secret. The installing team resolves those names
//! against its own `runners:`; `declares` is validated as a hard
//! ceiling by `check`, not just documented here. This module only
//! parses the manifest — resolving `requires` against local config and
//! enforcing `declares` are the engine's own, separate jobs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{PackName, PackRef, Publisher, RunnerName};
use crate::workflow::NodePermissions;

/// The pack's own identity: `publisher/name`, invoked as
/// `yunta run acme/review`, composed as `use: acme/qa-review`,
/// referenced as `skills: [acme/review-rubric]`. Kept as two plain
/// fields rather than one combined id — `publisher` and `name` are
/// independently meaningful (namespacing, `permissions.packs.publishers`
/// matches on `publisher` alone) and forcing every caller to split a
/// combined string back apart would just move the parsing problem
/// around instead of solving it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    pub name: PackName,
    pub publisher: Publisher,
    /// Semver — no comparison logic lives here; a pack's own
    /// version is never auto-resolved (`update` always names an exact
    /// target ref), so nothing in v1 needs to *compare* versions,
    /// only record and display them. Kept a plain string rather than a
    /// `semver` dependency for the same reason `yunta_schema` (below)
    /// stays a hand-rolled range check instead of one (see
    /// `engine/src/check.rs`'s own `yunta_schema_satisfied` doc comment)
    /// — add the dependency the day something actually needs to compare
    /// two versions, not ahead of that need.
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// `">=1 <2"` — the same comparator-range syntax and the same
    /// binary schema major (`yunta_core::YUNTA_SCHEMA`) a workflow's own
    /// `yunta_schema:` is checked against; `check_yunta_schema`'s
    /// range parser is reused verbatim once compatibility is actually
    /// enforced, not duplicated here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<String>,
    #[serde(default)]
    pub requires: PackRequires,
    pub declares: PackDeclares,
    #[serde(default)]
    pub contents: PackContents,
}

/// What the installer's own config must provide — never satisfied
/// by the pack itself; `check`/`add` validate these against the local
/// merged config, this type only parses what's asked for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRequires {
    #[serde(default)]
    pub runners: Vec<RequiredRunner>,
    /// Names the installer must define under its own `mcp_servers:`.
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    /// Binaries the pack's `bash` nodes assume are on `PATH`.
    #[serde(default)]
    pub commands: Vec<String>,
}

/// One entry in `requires.runners` — a runner name plus the ceiling of
/// permissions the pack asks that runner be resolvable with.
/// `permissions` absent means the pack doesn't care what profile the
/// runner resolves under, only that the name itself is defined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredRunner {
    pub name: RunnerName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<NodePermissions>,
}

/// The pack's own permissions **ceiling** — `check` rejects any
/// node, hook or criterion inside the pack that would exceed this,
/// error not warning; this type only carries the declared
/// values, it enforces nothing itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackDeclares {
    pub permissions: NodePermissions,
    /// The pack's `bash`/`command` nodes never assume network access —
    /// declarative only, mirrors `NetworkPermissions`'s own "no
    /// enforcement, audit-only" stance: this field is
    /// carried for `pack audit` and org policy
    /// (`permissions.packs`), the engine itself never blocks on it.
    #[serde(default)]
    pub network: bool,
    /// Executable code shipped with the pack — empty means the pack is
    /// 100% declarative (YAML/prompts/skills only), auditable by
    /// reading. A non-empty list requires explicit confirmation on
    /// `add` and gets flagged separately by `pack audit`.
    #[serde(default)]
    pub executors: Vec<String>,
}

/// What the pack physically ships — paths relative to the pack's
/// own root, resolved by whoever vendors it, not by this type.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackContents {
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    /// A pack can be knowledge-only (an org
    /// knowledge-pack layer) — an empty `workflows`/`skills` with a
    /// non-empty `knowledge` is a legitimate pack, not a degenerate one.
    #[serde(default)]
    pub knowledge: Vec<String>,
    #[serde(default)]
    pub docs: Vec<String>,
}

/// One `yunta.lock` entry: `{pack, ref, commit hash, content
/// hash}`. Nothing auto-updates — `update` always names an exact target
/// ref — so this is purely the record of exactly what got vendored, and
/// what an offline `add`/CI verifies the vendoring on disk against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackLockEntry {
    pub publisher: Publisher,
    pub name: PackName,
    /// The clone URL `add` was given — remembered so `update
    /// publisher/name@newref` never needs the URL
    /// repeated; only the ref changes.
    pub source: String,
    /// The ref `add`/`update` was told to install (a tag, branch or
    /// commit-ish) — what a human reads to know what was asked for.
    pub r#ref: String,
    /// The exact commit the ref resolved to at install time — what was
    /// actually cloned, independent of whether `ref` later moves (a
    /// branch does; a tag by convention shouldn't, but nothing here
    /// trusts that).
    pub commit: String,
    /// Content hash of the vendored tree (sha256 over sorted
    /// relative-path + file-content pairs, `.git` excluded) — what an
    /// offline `add`/CI verifies the vendoring on disk against,
    /// independent of git metadata surviving the copy.
    pub content_hash: String,
}

/// `yunta.lock` — every vendored pack, keyed by its [`PackRef`]
/// (`publisher/name`). A `BTreeMap` (not a `Vec`) so the file
/// serializes in a stable, diffable order regardless of install order —
/// the same reasoning a `Cargo.lock`-style file always wants.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackLock {
    #[serde(default)]
    pub packs: BTreeMap<PackRef, PackLockEntry>,
}

/// A manifest field whose value would reach outside the pack once it
/// is vendored under `.yunta/packs/<publisher>/<name>/`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackManifestError {
    #[error("`{field}` names `{value}` — every path in `contents` stays inside the pack: relative, with no `..` component")]
    PathEscapes { field: &'static str, value: String },
}

/// Whether `path` stays inside the directory it is relative to: not
/// absolute, and no `..` component anywhere.
pub fn stays_inside(path: &str) -> bool {
    let path = std::path::Path::new(path);
    !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

impl PackManifest {
    /// The pack's identity, `publisher/name`.
    pub fn reference(&self) -> PackRef {
        PackRef::new(self.publisher.clone(), self.name.clone())
    }

    /// Every path the manifest declares, checked against the place the
    /// pack is vendored to — all violations at once. `publisher` and
    /// `name` are checked by their own types when the manifest parses.
    pub fn validate(&self) -> Vec<PackManifestError> {
        let mut errors = Vec::new();
        let contents = &self.contents;
        for (field, paths) in [
            ("contents.workflows", &contents.workflows),
            ("contents.skills", &contents.skills),
            ("contents.knowledge", &contents.knowledge),
            ("contents.docs", &contents.docs),
        ] {
            for path in paths {
                if !stays_inside(path) {
                    errors.push(PackManifestError::PathEscapes {
                        field,
                        value: path.clone(),
                    });
                }
            }
        }
        errors
    }
}
