//! `yunta pack add/remove/list/update`: the command layer — argument
//! handling and printing — over `crate::pack`'s git/filesystem
//! mechanics. `add` and `update` also enforce `permissions.packs`: the
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

use yunta_core::{
    ConfigLayer, PackExecutorPolicy, PackLockEntry, PackManifest, PackRef, Publisher,
};
use yunta_engine::audit_pack;

use super::pack_audit::{count_pack_tests, print_report, print_test_summary, run_pack_tests};
use crate::error::{warn, CliError, Outcome};
use crate::pack::{
    clone_pack, clone_url, current_branch, hash_tree, head_commit, load_lock, lock_path,
    packs_root, read_manifest, save_lock, split_source_and_ref, staging_dir, vendor_dir,
    vendor_into_place,
};

/// The merged `permissions.packs` verdicts, with the layers that
/// declare each restriction kept by name — a refusal that can't say
/// *which* config file to change isn't actionable, since a permissions
/// ceiling set by a higher layer may name a file the user can't even
/// edit.
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
         (declared by the {} config layer{}) — allowed: {}. Add the publisher there, or \
         install a pack from an allowed publisher.",
        declared_by.join("/"),
        if declared_by.len() == 1 { "" } else { "s" },
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
            "this pack declares {} executor(s) and `permissions.packs.executors` \
             is `deny` (declared by the {} config layer{}) — `--yes` cannot override a \
             permissions ceiling. Change the policy there, or {verb} a pack \
             without executors.",
            manifest.declares.executors.len(),
            policy.executors_declared_by.join("/"),
            if policy.executors_declared_by.len() == 1 {
                ""
            } else {
                "s"
            },
        ))),
        PackExecutorPolicy::Prompt => {
            if confirmed {
                return Ok(());
            }
            Err(CliError::msg(format!(
                "this pack declares {} executor(s) — executable code, not just \
                 declarative YAML. Review the inventory above, then re-run with `--yes` to \
                 confirm the {verb}.",
                manifest.declares.executors.len()
            )))
        }
    }
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
) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;

    let (source_part, ref_arg) = split_source_and_ref(source);
    let url = clone_url(source_part);

    let clone_dir = tempfile::tempdir().map_err(|source| CliError::Io {
        context: "create a temp directory to clone into".to_string(),
        source,
    })?;

    clone_pack(&url, ref_arg, clone_dir.path()).await?;

    let manifest = read_manifest(clone_dir.path())?;

    let pack = manifest.reference();
    let dest = vendor_dir(&cwd, &pack);
    if dest.exists() {
        return Err(CliError::msg(format!(
            "`{pack}` is already installed at {} — use `yunta pack update` to change its \
             ref, or `yunta pack remove` first",
            dest.display()
        )));
    }

    // Policy first: a refused publisher or a denied executor leaves no
    // decision for a person to make, so the audit is not even printed.
    let policy = load_pack_policy(&cwd)?;
    enforce_publisher_allowed(&policy, &manifest.publisher)?;
    if policy.executors == PackExecutorPolicy::Deny && !manifest.declares.executors.is_empty() {
        enforce_executor_policy(&policy, &manifest, confirmed_executors, "install")?;
    }

    // The audit is read, never run: the inventory a person confirms
    // against, printed before anything is vendored.
    let audit = audit_pack(clone_dir.path(), manifest.clone());
    print_report(&audit);
    println!();
    enforce_executor_policy(&policy, &manifest, confirmed_executors, "install")?;

    let commit = head_commit(clone_dir.path()).await?;
    let resolved_ref = match ref_arg {
        Some(ref_arg) => ref_arg.to_string(),
        None => current_branch(clone_dir.path()).await?,
    };

    // The lock is loaded before anything is written, so a lock that
    // cannot be read stops the install before it starts.
    let mut lock = load_lock(&cwd)?;

    let staging = staging_dir(&cwd, &pack);
    vendor_into_place(clone_dir.path(), &staging, &dest)?;
    let content_hash = match hash_tree(&dest) {
        Ok(hash) => hash,
        Err(e) => {
            roll_back_vendored(&dest);
            return Err(e.into());
        }
    };

    lock.packs.insert(
        pack.clone(),
        PackLockEntry {
            publisher: manifest.publisher.clone(),
            name: manifest.name.clone(),
            source: url,
            r#ref: resolved_ref,
            commit: commit.clone(),
            content_hash,
        },
    );
    if let Err(e) = save_lock(&cwd, &lock) {
        roll_back_vendored(&dest);
        return Err(e.into());
    }

    println!(
        "installed {}/{} @ {} ({}) -> {}",
        manifest.publisher,
        manifest.name,
        manifest.version,
        &commit[..commit.len().min(12)],
        dest.display()
    );

    // The pack's own cases run last, on the vendored copy, and only on
    // request — after the confirmation, never as part of deciding it.
    let tests = if run_tests {
        run_pack_tests(&dest).await
    } else {
        count_pack_tests(&dest)
    };
    print_test_summary(&tests);
    Ok(Outcome::Success)
}

/// Removes a tree this command vendored moments ago, when a later step
/// failed: an install that did not complete leaves nothing behind.
fn roll_back_vendored(dest: &std::path::Path) {
    if let Err(e) = std::fs::remove_dir_all(dest) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn(format!(
                "could not remove the vendored tree at {} after the failed install: {e}",
                dest.display()
            ));
        }
    }
}

pub async fn update(
    pack: &PackRef,
    new_ref: &str,
    confirmed_executors: bool,
) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let mut lock = load_lock(&cwd)?;
    let Some(entry) = lock.packs.get(pack).cloned() else {
        return Err(CliError::msg(format!(
            "`{pack}` isn't installed — `yunta pack add` it first, not update"
        )));
    };

    let clone_dir = tempfile::tempdir().map_err(|source| CliError::Io {
        context: "create a temp directory to clone into".to_string(),
        source,
    })?;
    clone_pack(&entry.source, Some(new_ref), clone_dir.path()).await?;
    let manifest = read_manifest(clone_dir.path())?;
    if manifest.reference() != *pack {
        return Err(CliError::msg(format!(
            "`{new_ref}` of `{}` identifies itself as `{}/{}`, not `{pack}` — \
             refusing to update a pack into a different one",
            entry.source, manifest.publisher, manifest.name
        )));
    }

    // The same policy gates as `add` — a new ref is where new executor
    // code first appears, and an allow-list narrowed since the install
    // must stop pulling from a publisher it no longer trusts.
    let policy = load_pack_policy(&cwd)?;
    enforce_publisher_allowed(&policy, pack.publisher())?;
    if !manifest.declares.executors.is_empty() {
        // The decision being demanded needs the same evidence `add`
        // shows — nothing executes until a human has seen the
        // inventory — printed only when there's a decision to make.
        print_report(&audit_pack(clone_dir.path(), manifest.clone()));
        println!();
    }
    enforce_executor_policy(&policy, &manifest, confirmed_executors, "update")?;

    let commit = head_commit(clone_dir.path()).await?;

    // The new ref is vendored beside the installed tree and swapped in
    // only once it is complete: a ref that cannot be vendored leaves the
    // installed pack, and the lock, exactly as they were.
    let dest = vendor_dir(&cwd, pack);
    let staging = staging_dir(&cwd, pack);
    vendor_into_place(clone_dir.path(), &staging, &dest)?;
    let content_hash = hash_tree(&dest)?;

    lock.packs.insert(
        pack.clone(),
        PackLockEntry {
            publisher: pack.publisher().clone(),
            name: pack.name().clone(),
            source: entry.source,
            r#ref: new_ref.to_string(),
            commit: commit.clone(),
            content_hash,
        },
    );
    save_lock(&cwd, &lock)?;

    println!(
        "updated {pack} -> {new_ref} ({})",
        &commit[..commit.len().min(12)]
    );
    Ok(Outcome::Success)
}

pub fn remove(pack: &PackRef) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let mut lock = load_lock(&cwd)?;
    if lock.packs.remove(pack).is_none() {
        return Err(CliError::msg(format!("`{pack}` isn't installed")));
    }

    let dest = vendor_dir(&cwd, pack);
    if let Err(source) = std::fs::remove_dir_all(&dest) {
        if source.kind() != std::io::ErrorKind::NotFound {
            return Err(CliError::io("remove", dest.display(), source));
        }
    }
    save_lock(&cwd, &lock)?;

    println!("removed {pack}");
    Ok(Outcome::Success)
}

pub fn list() -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let lock = load_lock(&cwd)?;
    if lock.packs.is_empty() {
        println!("no packs installed under {}", packs_root(&cwd).display());
        return Ok(Outcome::Success);
    }

    // Reproducible offline installs mean verifying against the lock:
    // re-hash what's actually vendored on disk and say so when it no
    // longer matches what the lock recorded, rather than just trusting
    // the lock's own numbers back at the user.
    for (key, entry) in &lock.packs {
        let dest = vendor_dir(&cwd, key);
        let status = match hash_tree(&dest) {
            Ok(hash) if hash == entry.content_hash => "ok".to_string(),
            Ok(_) => "MODIFIED (vendored content no longer matches the lock)".to_string(),
            Err(_) => "MISSING (vendored directory not found)".to_string(),
        };
        println!(
            "{key} @ {} ({}) — {status}",
            entry.r#ref,
            &entry.commit[..entry.commit.len().min(12)]
        );
    }
    println!("lock: {}", lock_path(&cwd).display());
    Ok(Outcome::Success)
}
