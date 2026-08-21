//! `pack.yaml` (RFC-0002 §3, T11.1): the manifest of a distributable,
//! versioned bundle of workflows/skills/knowledge/docs (and, optionally,
//! executors). A pack never extends the engine — it only ships material
//! the engine already knows how to run, the same relationship a Helm
//! chart or a Terraform module has to its runtime.
//!
//! Normative split this type exists to enforce: a pack **declares**
//! roles (capabilities + permissions it needs) and a permissions
//! **ceiling** (`declares`) it promises never to exceed — never a
//! runner, a binding, a concrete model or a secret. The installing team
//! resolves those roles against its own `runners:`; `declares` is
//! validated as a hard ceiling by `check` (T11.5), not just documented
//! here. This module only parses the manifest — resolving `requires`
//! against local config (T11.6) and enforcing `declares` (T11.5) are
//! the engine's own, separate jobs.

use serde::{Deserialize, Serialize};

use crate::workflow::NodePermissions;

/// The pack's own identity: `publisher/name`, invoked as
/// `yunta run acme/review`, composed as `use: acme/qa-review`,
/// referenced as `skills: [acme/review-rubric]` (§5). Kept as two plain
/// fields rather than one combined id — `publisher` and `name` are
/// independently meaningful (namespacing, `permissions.packs.publishers`
/// matches on `publisher` alone) and forcing every caller to split a
/// combined string back apart would just move the parsing problem
/// around instead of solving it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackManifest {
    pub name: String,
    pub publisher: String,
    /// Semver (§3, §7) — no comparison logic lives here; a pack's own
    /// version is never auto-resolved (`update` always names an exact
    /// target ref, §4), so nothing in v1 needs to *compare* versions,
    /// only record and display them. Kept a plain string rather than a
    /// `semver` dependency for the same reason `yunta_schema` (below)
    /// stays a hand-rolled range check instead of one (D-precedent:
    /// `engine/src/check.rs`'s own `yunta_schema_satisfied` doc comment)
    /// — add the dependency the day something actually needs to compare
    /// two versions, not ahead of that need.
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// `">=1 <2"` (§7) — the same comparator-range syntax and the same
    /// binary schema major (`yunta_core::YUNTA_SCHEMA`) a workflow's own
    /// `yunta_schema:` is checked against; `check_yunta_schema`'s
    /// range parser is reused verbatim once compatibility is actually
    /// enforced (T11.6), not duplicated here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<String>,
    #[serde(default)]
    pub requires: PackRequires,
    pub declares: PackDeclares,
    #[serde(default)]
    pub contents: PackContents,
}

/// What the installer's own config must provide (§3) — never satisfied
/// by the pack itself; `check`/`add` validate these against the local
/// merged config (T11.6), this type only parses what's asked for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackRequires {
    #[serde(default)]
    pub roles: Vec<RequiredRole>,
    /// Names the installer must define under its own `mcp_servers:`.
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    /// Binaries the pack's `bash` nodes assume are on `PATH`.
    #[serde(default)]
    pub commands: Vec<String>,
}

/// One entry in `requires.roles` (§3) — a role name plus the ceiling of
/// permissions the pack asks that role be resolvable with. `permissions`
/// absent means the pack doesn't care what profile the role resolves
/// under, only that the role itself exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequiredRole {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<NodePermissions>,
}

/// The pack's own permissions **ceiling** (§3) — `check` rejects any
/// node, hook or criterion inside the pack that would exceed this,
/// error not warning (T11.5); this type only carries the declared
/// values, it enforces nothing itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackDeclares {
    pub permissions: NodePermissions,
    /// The pack's `bash`/`command` nodes never assume network access —
    /// declarative only, mirrors `NetworkPermissions`'s own "no
    /// enforcement, audit-only" stance (D105/§6.1): this field is
    /// carried for `pack audit` (T11.4) and org policy
    /// (`permissions.packs`), the engine itself never blocks on it.
    #[serde(default)]
    pub network: bool,
    /// Executable code shipped with the pack — empty means the pack is
    /// 100% declarative (YAML/prompts/skills only), auditable by
    /// reading. A non-empty list requires explicit confirmation on
    /// `add` (§6.3) and gets flagged separately by `pack audit`.
    #[serde(default)]
    pub executors: Vec<String>,
}

/// What the pack physically ships (§3) — paths relative to the pack's
/// own root, resolved by whoever vendors it (T11.2), not by this type.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackContents {
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    /// A pack can be knowledge-only (Contrato §9.2, D56's org
    /// knowledge-pack layer) — an empty `workflows`/`skills` with a
    /// non-empty `knowledge` is a legitimate pack, not a degenerate one.
    #[serde(default)]
    pub knowledge: Vec<String>,
    #[serde(default)]
    pub docs: Vec<String>,
}
