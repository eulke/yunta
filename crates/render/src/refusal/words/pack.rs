//! See [`super`]. The sentences for what a pack command refuses: what the
//! pack is, where it stands and in which store, and the policy that stops it.

use std::path::Path;

use crate::command;
use yunta_core::refusal::{PackRefusal, PackVerb};
use yunta_core::step::{Arg, NextStep};
use yunta_core::{PackRef, PackScope};

/// Why a pack command refuses, in the words of the command refused: what the
/// pack is or where it stands, the policy that stops it, and the lines that
/// go on where there are some.
pub(in crate::refusal) fn pack(refusal: &PackRefusal) -> String {
    match refusal {
        PackRefusal::AlreadyInstalled { pack, at, scope } => already_installed(pack, at, *scope),
        PackRefusal::NotInstalled {
            pack,
            scope,
            found_in,
        } => not_installed(pack, *scope, *found_in),
        PackRefusal::NothingToRemove {
            pack,
            scope,
            found_in,
        } => {
            let remove = |scope| NextStep::PackRemove {
                pack: pack.clone(),
                scope,
            };
            let there = elsewhere(*found_in, remove);
            format!("`{pack}` {}{there}", isnt_installed(*scope))
        }
        PackRefusal::NotVendored {
            pack,
            root,
            found_in,
            ..
        } => {
            let audit = |scope| NextStep::PackAudit {
                pack: pack.clone(),
                scope,
            };
            let there = elsewhere(*found_in, audit);
            format!("`{pack}` isn't installed under {}{there}", root.display())
        }
        PackRefusal::SchemaOutside {
            pack,
            range,
            speaks,
        } => format!(
            "`{pack}` declares `yunta_schema: \"{range}\"`, and this binary speaks schema \
             {speaks}. Install a version of yunta inside that range, or a release of the pack \
             that accepts this one."
        ),
        PackRefusal::PublisherNotAllowed {
            publisher,
            allowed,
            declared_by,
        } => format!(
            "publisher `{publisher}` is not in `permissions.packs.publishers.allow` \
             (declared by the {} config {}); allowed: {}. Add the publisher there, or \
             install a pack from an allowed publisher.",
            declared_by.join("/"),
            surco::text::agreeing(declared_by.len(), "layer", "layers"),
            allowed
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PackRefusal::ExecutorsDenied {
            executors,
            declared_by,
            verb,
        } => format!(
            "this pack declares {} and `permissions.packs.executors` \
             is `deny` (declared by the {} config {}): `--yes` cannot override a \
             permissions ceiling. Change the policy there, or {} a pack \
             without executors.",
            surco::text::counted(*executors, "executor"),
            declared_by.join("/"),
            surco::text::agreeing(declared_by.len(), "layer", "layers"),
            pack_verb(*verb),
        ),
        PackRefusal::ExecutorsUnconfirmed { executors, verb } => format!(
            "this pack declares {}: executable code, not just \
             declarative YAML. Review the inventory above, then re-run with `--yes` to \
             confirm the {}.",
            surco::text::counted(*executors, "executor"),
            pack_verb(*verb),
        ),
        PackRefusal::AnotherPack {
            pack,
            reference,
            cloned_from,
            is,
        } => format!(
            "`{reference}` of `{cloned_from}` identifies itself as `{is}`, not `{pack}`; \
             refusing to update a pack into a different one"
        ),
        PackRefusal::DirectoryExists { dir } => format!(
            "{} already exists; choose another name or remove it first",
            dir.display()
        ),
    }
}

/// What a pack command does, as its refusal says it.
fn pack_verb(verb: PackVerb) -> &'static str {
    match verb {
        PackVerb::Install => "install",
        PackVerb::Update => "update",
    }
}

/// Why `add` refuses `pack`, installed already at `dest` in the store `scope`
/// names: the lines that change its ref there, or take it out of it first.
fn already_installed(pack: &PackRef, dest: &Path, scope: PackScope) -> String {
    let update = NextStep::PackUpdate {
        pack: pack.clone(),
        reference: Arg::Blank,
        scope,
    };
    let remove = NextStep::PackRemove {
        pack: pack.clone(),
        scope,
    };
    format!(
        "`{pack}` is already installed at {}; use `{}` to change its ref, or `{}` first",
        dest.display(),
        command::line(&update),
        command::line(&remove)
    )
}

/// Why `update` refuses `pack`, which the store `scope` names does not hold: the
/// line that updates it in the other store when that one holds it, else the
/// line that adds it here.
fn not_installed(pack: &PackRef, scope: PackScope, found_in: Option<PackScope>) -> String {
    if found_in.is_some() {
        // A second install would shadow the copy the other store holds.
        let update = |scope| NextStep::PackUpdate {
            pack: pack.clone(),
            reference: Arg::Blank,
            scope,
        };
        let there = elsewhere(found_in, update);
        return format!("`{pack}` {}{there}", isnt_installed(scope));
    }
    let add = NextStep::PackAdd {
        source: Arg::Blank,
        scope,
    };
    format!(
        "`{pack}` {}; `{}` it first, not update",
        isnt_installed(scope),
        command::line(&add)
    )
}

/// Where a pack the store asked lacks is, when the other store holds it, and
/// the command `step` gives that reaches it there. Nothing when no store
/// holds it.
fn elsewhere(found_in: Option<PackScope>, step: impl Fn(PackScope) -> NextStep) -> String {
    let Some(scope) = found_in else {
        return String::new();
    };
    let held = match scope {
        PackScope::Project => "it is installed in this project",
        PackScope::User => "it is among the user's packs",
    };
    format!("; {held}: `{}`", command::line(&step(scope)))
}

/// That a pack is not in the store `scope` names: not installed in this
/// project, or not among the user's packs, which every project reads.
fn isnt_installed(scope: PackScope) -> &'static str {
    match scope {
        PackScope::Project => "isn't installed",
        PackScope::User => "isn't among the user's packs",
    }
}
