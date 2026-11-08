//! `yunta pack add/new/remove/list/update/audit`: the command layer —
//! argument handling and printing — over `crate::pack`'s git/filesystem
//! mechanics. Its pages are `page`'s, the audit is `audit`, what a vendored
//! pack needs from the project is `needs`, and `new` is `scaffold`. `add` and `update` also enforce `permissions.packs`: the
//! publisher allow-list refuses a source outside it, and the
//! `executors: allow|prompt|deny` policy governs the confirmation
//! gate — `prompt` (the default) requires `--yes` after the audit has
//! shown exactly what the executors are, `deny` refuses even with it,
//! `allow` installs without asking. `update` gates too: a new ref is
//! where new executor code first appears, so a gate on `add` alone
//! would be governance theater. Neither command runs anything of the
//! pack on its own: its test cases run only behind `--run-tests`, after
//! the install, and every write goes through a staging directory or a
//! temporary file so a failure leaves the repository as it was.

use std::path::{Path, PathBuf};

use surco::components::Found;
use surco::look::Look;
use yunta_core::{
    ConfigLayer, ContentHash, PackExecutorPolicy, PackLock, PackLockEntry, PackManifest, PackRef,
    Publisher,
};
use yunta_engine::audit_pack;

use crate::context::Context;
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{blank, show};
use crate::pack::{
    clone_pack, clone_url, current_branch, hash_tree, head_commit, load_lock, lock_path,
    packs_root, read_manifest, save_lock, split_source_and_ref, staging_dir, vendor_dir,
    vendor_into_place, with_cleanup, PackError, Swap,
};

mod audit;
mod needs;
mod page;
mod scaffold;

pub use audit::audit;
use audit::{count_pack_tests, run_pack_tests};
use page::{Installed, Inventory, Listed, Removed, Updated, VendoredPack};
pub use scaffold::new_pack;

/// The merged `permissions.packs` verdicts, with the layers that
/// declare each restriction kept by name — a refusal that can't say
/// *which* config file to change isn't actionable, since a permissions
/// ceiling set by a higher layer may name a file the user can't even
/// edit.
/// Whether the executors `manifest` declares wait on a person's `--yes`
/// before the pack is vendored: under a policy that prompts, until they
/// confirm.
fn waits_on_yes(policy: &PackPolicy, manifest: &PackManifest, confirmed: bool) -> bool {
    !manifest.declares.executors.is_empty()
        && policy.executors == PackExecutorPolicy::Prompt
        && !confirmed
}

struct PackPolicy {
    /// Non-empty allow-list, with the declaring layer names — `None`
    /// when no layer restricts publishers (empty = everyone).
    publishers_allow: Option<(Vec<Publisher>, Vec<&'static str>)>,
    /// The merged (strictest-wins) executor policy; `Prompt` when no
    /// layer declares one — the longstanding default behavior.
    executors: PackExecutorPolicy,
    /// Layers declaring the policy that ended up winning the merge.
    executors_declared_by: Vec<&'static str>,
}

fn load_pack_policy(cwd: &std::path::Path) -> Result<PackPolicy, CliError> {
    let layers = crate::project::load_named_layers(cwd)?;
    let merged = ConfigLayer::merge_layers(layers.iter().map(|(_, layer)| layer.clone()));
    let packs = merged
        .permissions
        .as_ref()
        .and_then(|permissions| permissions.packs.as_ref());

    let allow = packs
        .and_then(|p| p.publishers.as_ref())
        .map(|p| p.allow.clone())
        .filter(|allow| !allow.is_empty());
    let publishers_allow = allow.map(|allow| {
        let declared_by = layers
            .iter()
            .filter(|(_, layer)| {
                layer
                    .permissions
                    .as_ref()
                    .and_then(|p| p.packs.as_ref())
                    .and_then(|p| p.publishers.as_ref())
                    .is_some_and(|p| !p.allow.is_empty())
            })
            .map(|(name, _)| *name)
            .collect();
        (allow, declared_by)
    });

    let executors = packs
        .and_then(|p| p.executors)
        .unwrap_or(PackExecutorPolicy::Prompt);
    let executors_declared_by = layers
        .iter()
        .filter(|(_, layer)| {
            layer
                .permissions
                .as_ref()
                .and_then(|p| p.packs.as_ref())
                .and_then(|p| p.executors)
                == Some(executors)
        })
        .map(|(name, _)| *name)
        .collect();

    Ok(PackPolicy {
        publishers_allow,
        executors,
        executors_declared_by,
    })
}

/// The schema gate: a pack states the schema major it needs, and a
/// binary outside that range cannot run its workflows. Refused before
/// anything is vendored — a pack on disk that no run can use is a
/// failure discovered later, with a tree to clean up.
fn enforce_schema_range(manifest: &PackManifest) -> Result<(), CliError> {
    let Some(range) = &manifest.yunta_schema else {
        return Ok(());
    };
    if range.holds_for(yunta_core::YUNTA_SCHEMA) {
        return Ok(());
    }
    Err(CliError::msg(format!(
        "`{}/{}` declares `yunta_schema: \"{range}\"`, and this binary speaks schema {}. \
         Install a version of yunta inside that range, or a release of the pack that \
         accepts this one.",
        manifest.publisher,
        manifest.name,
        yunta_core::YUNTA_SCHEMA,
    )))
}

/// The publisher allow-list gate: a non-empty
/// `permissions.packs.publishers.allow` in the merged config refuses
/// any publisher outside it, naming the declaring layer(s).
fn enforce_publisher_allowed(policy: &PackPolicy, publisher: &Publisher) -> Result<(), CliError> {
    let Some((allow, declared_by)) = &policy.publishers_allow else {
        return Ok(());
    };
    if allow.iter().any(|allowed| allowed == publisher) {
        return Ok(());
    }
    Err(CliError::msg(format!(
        "publisher `{publisher}` is not in `permissions.packs.publishers.allow` \
         (declared by the {} config {}); allowed: {}. Add the publisher there, or \
         install a pack from an allowed publisher.",
        declared_by.join("/"),
        surco::text::agreeing(declared_by.len(), "layer", "layers"),
        allow
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

/// The executor policy gate, generalizing a fixed `--yes` into a
/// configurable policy: `deny` refuses regardless of confirmation — a
/// ceiling a flag must never override — `prompt` requires `--yes`,
/// `allow` passes. Only consulted when the manifest actually declares
/// executors.
fn enforce_executor_policy(
    policy: &PackPolicy,
    manifest: &PackManifest,
    confirmed: bool,
    verb: &str,
) -> Result<(), CliError> {
    if manifest.declares.executors.is_empty() {
        return Ok(());
    }
    match policy.executors {
        PackExecutorPolicy::Allow => Ok(()),
        PackExecutorPolicy::Deny => Err(CliError::msg(format!(
            "this pack declares {} and `permissions.packs.executors` \
             is `deny` (declared by the {} config {}): `--yes` cannot override a \
             permissions ceiling. Change the policy there, or {verb} a pack \
             without executors.",
            surco::text::counted(manifest.declares.executors.len(), "executor"),
            policy.executors_declared_by.join("/"),
            surco::text::agreeing(policy.executors_declared_by.len(), "layer", "layers"),
        ))),
        PackExecutorPolicy::Prompt => {
            if confirmed {
                return Ok(());
            }
            Err(CliError::msg(format!(
                "this pack declares {}: executable code, not just \
                 declarative YAML. Review the inventory above, then re-run with `--yes` to \
                 confirm the {verb}.",
                surco::text::counted(manifest.declares.executors.len(), "executor")
            )))
        }
    }
}

/// Vendors the clone at `clone` as `pack` through a staging directory and
/// records it in `lock` with the entry `entry` makes of its content hash,
/// the lock written atomically. A tree that cannot be vendored leaves the
/// installed pack and the lock exactly as they were, and a failure after
/// the swap puts the installed pack back. Answers where the pack is.
fn vendor_and_lock(
    cwd: &Path,
    clone: &Path,
    pack: &PackRef,
    mut lock: PackLock,
    entry: impl FnOnce(ContentHash) -> PackLockEntry,
) -> Result<PathBuf, CliError> {
    let dest = vendor_dir(cwd, pack);
    let staging = staging_dir(cwd, pack);
    let swap = vendor_into_place(clone, &staging, &dest)?;
    let content_hash = match hash_tree(&dest) {
        Ok(hash) => hash,
        Err(cause) => return Err(with_cleanup(cause, swap.undo()).into()),
    };
    lock.packs.insert(pack.clone(), entry(content_hash));
    if let Err(cause) = save_lock(cwd, &lock) {
        return Err(with_cleanup(cause, swap.undo()).into());
    }
    swap.commit();
    Ok(dest)
}

/// `yunta pack add`, in the order that keeps a person in charge:
/// policy (publisher allow-list, executor policy) → audit → confirmation
/// → vendor through a staging directory → lock, written atomically →
/// the pack's own tests, only with `--run-tests`. Nothing of the pack
/// executes before the confirmation, and a failure at any step leaves
/// the repository exactly as it was.
pub async fn add(
    source: &str,
    confirmed_executors: bool,
    run_tests: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    // The invocation's own composition root: `pack add` clones, and a
    // clone is a subprocess that answers to whoever ran the command.
    let ctx = Context::load()?;
    let cwd = ctx.cwd.clone();

    let (source_part, ref_arg) = split_source_and_ref(source);
    let url = clone_url(source_part);

    let clone_dir = tempfile::tempdir().map_err(|source| CliError::Io {
        context: "create a temp directory to clone into".to_string(),
        source,
    })?;

    clone_pack(&url, ref_arg, clone_dir.path(), ctx.supervision()).await?;

    let manifest = read_manifest(clone_dir.path())?;

    let pack = manifest.reference();
    let dest = vendor_dir(&cwd, &pack);
    if dest.exists() {
        return Err(CliError::msg(page::already_installed(&pack, &dest)));
    }

    // Policy first: a refused publisher or a denied executor leaves no
    // decision for a person to make, so the audit is not even printed.
    enforce_schema_range(&manifest)?;
    let policy = load_pack_policy(&cwd)?;
    enforce_publisher_allowed(&policy, &manifest.publisher)?;
    if policy.executors == PackExecutorPolicy::Deny && !manifest.declares.executors.is_empty() {
        enforce_executor_policy(&policy, &manifest, confirmed_executors, "install")?;
    }

    // The audit is read, never run: the inventory a person confirms
    // against, shown before anything is vendored.
    let audit = audit_pack(clone_dir.path(), manifest.clone());
    let inventory = Inventory::Installing {
        audit: &audit,
        waits_on_yes: waits_on_yes(&policy, &manifest, confirmed_executors),
    };
    show(look, &inventory);
    blank();
    enforce_executor_policy(&policy, &manifest, confirmed_executors, "install")?;

    let commit = head_commit(clone_dir.path(), ctx.supervision()).await?;
    let resolved_ref = match ref_arg {
        Some(ref_arg) => ref_arg.to_string(),
        None => current_branch(clone_dir.path(), ctx.supervision()).await?,
    };

    // The lock is loaded before anything is written, so a lock that
    // cannot be read stops the install before it starts.
    let lock = load_lock(&cwd)?;
    let dest = vendor_and_lock(&cwd, clone_dir.path(), &pack, lock, |content_hash| {
        PackLockEntry {
            publisher: manifest.publisher.clone(),
            name: manifest.name.clone(),
            source: url,
            r#ref: resolved_ref,
            commit: commit.clone(),
            content_hash,
        }
    })?;

    // The pack's own cases run last, on the vendored copy, and only on
    // request: after the confirmation, never as part of deciding it.
    let tests = match run_tests {
        true => run_pack_tests(&dest, ctx.interrupt()).await,
        false => count_pack_tests(&dest),
    };
    let needs = needs::unmet(&ctx, &dest, &manifest).await;
    let installed = |needs| Installed {
        manifest: &manifest,
        commit: &commit,
        at: &dest,
        cwd: &cwd,
        tests: &tests,
        needs,
    };
    // The pack is installed whatever stopped the reading of its needs.
    let needs = needs.inspect_err(|_| show(look, &installed(Vec::new())))?;
    show(look, &installed(needs));
    Ok(Outcome::Success)
}

pub async fn update(
    pack: &PackRef,
    new_ref: &str,
    confirmed_executors: bool,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let cwd = ctx.cwd.clone();
    let lock = load_lock(&cwd)?;
    let Some(entry) = lock.packs.get(pack).cloned() else {
        return Err(CliError::msg(page::not_installed(pack)));
    };

    let clone_dir = tempfile::tempdir().map_err(|source| CliError::Io {
        context: "create a temp directory to clone into".to_string(),
        source,
    })?;
    clone_pack(
        &entry.source,
        Some(new_ref),
        clone_dir.path(),
        ctx.supervision(),
    )
    .await?;
    let manifest = read_manifest(clone_dir.path())?;
    if manifest.reference() != *pack {
        return Err(CliError::msg(format!(
            "`{new_ref}` of `{}` identifies itself as `{}/{}`, not `{pack}`; \
             refusing to update a pack into a different one",
            entry.source, manifest.publisher, manifest.name
        )));
    }

    // The same policy gates as `add` — a new ref is where new executor
    // code first appears, and an allow-list narrowed since the install
    // must stop pulling from a publisher it no longer trusts.
    enforce_schema_range(&manifest)?;
    let policy = load_pack_policy(&cwd)?;
    enforce_publisher_allowed(&policy, pack.publisher())?;
    if policy.executors == PackExecutorPolicy::Deny && !manifest.declares.executors.is_empty() {
        enforce_executor_policy(&policy, &manifest, confirmed_executors, "update")?;
    }
    if !manifest.declares.executors.is_empty() {
        // The decision being demanded needs the same evidence `add`
        // shows: nothing executes until a person has seen the inventory,
        // shown only when there is a decision to make.
        let audit = audit_pack(clone_dir.path(), manifest.clone());
        let inventory = Inventory::Updating {
            audit: &audit,
            waits_on_yes: waits_on_yes(&policy, &manifest, confirmed_executors),
        };
        show(look, &inventory);
        blank();
    }
    enforce_executor_policy(&policy, &manifest, confirmed_executors, "update")?;

    let commit = head_commit(clone_dir.path(), ctx.supervision()).await?;

    // The new ref is vendored beside the installed tree and swapped in
    // only once it is complete.
    vendor_and_lock(&cwd, clone_dir.path(), pack, lock, |content_hash| {
        PackLockEntry {
            publisher: pack.publisher().clone(),
            name: pack.name().clone(),
            source: entry.source,
            r#ref: new_ref.to_string(),
            commit: commit.clone(),
            content_hash,
        }
    })?;

    let updated = Updated {
        pack,
        to: new_ref,
        commit: &commit,
    };
    show(look, &updated);
    Ok(Outcome::Success)
}

pub fn remove(pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let mut lock = load_lock(&cwd)?;
    if lock.packs.remove(pack).is_none() {
        return Err(CliError::msg(format!("`{pack}` isn't installed")));
    }

    // The tree moves aside and is deleted only once the lock without it is
    // saved: a failed save puts it back.
    let dest = vendor_dir(&cwd, pack);
    let aside = Swap::aside(&dest)?;
    if let Err(cause) = save_lock(&cwd, &lock) {
        return Err(with_cleanup(cause, aside.map_or(Ok(()), Swap::undo)).into());
    }
    if let Some(swap) = aside {
        swap.commit();
    }

    show(look, &Removed { pack });
    Ok(Outcome::Success)
}

/// What a vendored tree says about its lock entry, as its `list` row states
/// it: whether its files still hash to the recorded content, or why they
/// cannot be read.
fn vendored_verdict(
    dest: &std::path::Path,
    entry: &PackLockEntry,
    installed: &str,
) -> (Found, String) {
    match hash_tree(dest) {
        Ok(hash) if hash == entry.content_hash => {
            (Found::Holds, format!("{installed}, as its lock records it"))
        }
        Ok(_) => (
            Found::Caution,
            format!("{installed}, modified: its files no longer match its lock"),
        ),
        // Only a directory that is not there counts as missing.
        Err(PackError::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => (
            Found::Problem,
            format!("{installed}, missing: its directory is not there"),
        ),
        Err(error) => (
            Found::Problem,
            format!(
                "{installed}, cannot be read: {}",
                yunta_core::describe(&error)
            ),
        ),
    }
}

pub fn list(look: &Look) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let lock = load_lock(&cwd)?;
    if lock.packs.is_empty() {
        let looked_in = surco::paths::shown(&packs_root(&cwd), &cwd, None);
        show(look, &Listed::Empty { looked_in });
        return Ok(Outcome::Success);
    }

    // Reproducible offline installs mean verifying against the lock:
    // re-hash what's actually vendored on disk and say so when it no
    // longer matches what the lock recorded, rather than just trusting
    // the lock's own numbers back at the user.
    let packs = lock
        .packs
        .iter()
        .map(|(key, entry)| {
            let dest = vendor_dir(&cwd, key);
            let installed = format!("{} ({})", entry.r#ref, entry.commit.abbreviated());
            let (found, said) = vendored_verdict(&dest, entry, &installed);
            VendoredPack {
                pack: key.to_string(),
                found,
                said,
            }
        })
        .collect();
    let lock = surco::paths::shown(&lock_path(&cwd), &cwd, None);
    show(look, &Listed::Locked { packs, lock });
    Ok(Outcome::Success)
}
