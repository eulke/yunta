//! `yunta pack add/remove/list/update`: the command layer — argument
//! handling and printing — over `crate::pack`'s git/filesystem
//! mechanics. `add` and `update` also enforce `permissions.packs`: the
//! publisher allow-list refuses a source outside it, and the
//! `executors: allow|prompt|deny` policy governs the confirmation
//! gate — `prompt` (the default) requires `--yes` after the audit has
//! shown exactly what the executors are, `deny` refuses even with it,
//! `allow` installs without asking. `update` gates too: a new ref is
//! where new executor code first appears, so a gate on `add` alone
//! would be governance theater.

use std::process::ExitCode;

use yunta_core::{ConfigLayer, PackExecutorPolicy, PackLockEntry, PackManifest};
use yunta_engine::audit_pack;

use super::pack_audit::{print_report, run_pack_tests};
use crate::pack::{
    clone_pack, clone_url, current_branch, hash_tree, head_commit, load_lock, lock_path,
    packs_root, read_manifest, save_lock, split_source_and_ref, vendor_dir, vendor_tree,
};

/// The merged `permissions.packs` verdicts, with the layers that
/// declare each restriction kept by name — a refusal that can't say
/// *which* config file to change isn't actionable, since a permissions
/// ceiling set by a higher layer may name a file the user can't even
/// edit.
struct PackPolicy {
    /// Non-empty allow-list, with the declaring layer names — `None`
    /// when no layer restricts publishers (empty = everyone).
    publishers_allow: Option<(Vec<String>, Vec<&'static str>)>,
    /// The merged (strictest-wins) executor policy; `Prompt` when no
    /// layer declares one — the longstanding default behavior.
    executors: PackExecutorPolicy,
    /// Layers declaring the policy that ended up winning the merge.
    executors_declared_by: Vec<&'static str>,
}

fn load_pack_policy(cwd: &std::path::Path) -> Result<PackPolicy, ExitCode> {
    let layers = match crate::project::load_named_layers(cwd) {
        Ok(layers) => layers,
        Err(e) => {
            eprintln!("error: {e}");
            return Err(ExitCode::FAILURE);
        }
    };
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
fn enforce_publisher_allowed(policy: &PackPolicy, publisher: &str) -> Result<(), ExitCode> {
    let Some((allow, declared_by)) = &policy.publishers_allow else {
        return Ok(());
    };
    if allow.iter().any(|allowed| allowed == publisher) {
        return Ok(());
    }
    eprintln!(
        "error: publisher `{publisher}` is not in `permissions.packs.publishers.allow` \
         (declared by the {} config layer{}) — allowed: {}. Add the publisher there, or \
         install a pack from an allowed publisher.",
        declared_by.join("/"),
        if declared_by.len() == 1 { "" } else { "s" },
        allow.join(", ")
    );
    Err(ExitCode::FAILURE)
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
) -> Result<(), ExitCode> {
    if manifest.declares.executors.is_empty() {
        return Ok(());
    }
    match policy.executors {
        PackExecutorPolicy::Allow => Ok(()),
        PackExecutorPolicy::Deny => {
            eprintln!(
                "error: this pack declares {} executor(s) and `permissions.packs.executors` \
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
            );
            Err(ExitCode::FAILURE)
        }
        PackExecutorPolicy::Prompt => {
            if confirmed {
                return Ok(());
            }
            eprintln!(
                "error: this pack declares {} executor(s) — executable code, not just \
                 declarative YAML. Review the inventory above, then re-run with `--yes` to \
                 confirm the {verb}.",
                manifest.declares.executors.len()
            );
            Err(ExitCode::FAILURE)
        }
    }
}

pub async fn add(source: &str, confirmed_executors: bool) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    let (source_part, ref_arg) = split_source_and_ref(source);
    let url = clone_url(source_part);

    let clone_dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("error: could not create a temp directory to clone into: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = clone_pack(&url, ref_arg, clone_dir.path()).await {
        eprintln!("error: could not clone `{url}`: {e}");
        return ExitCode::FAILURE;
    }

    let manifest = match read_manifest(clone_dir.path()) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let dest = vendor_dir(&cwd, &manifest.publisher, &manifest.name);
    if dest.exists() {
        eprintln!(
            "error: `{}/{}` is already installed at {} — use `yunta pack update` to change its \
             ref, or `yunta pack remove` first",
            manifest.publisher,
            manifest.name,
            dest.display()
        );
        return ExitCode::FAILURE;
    }

    // The publisher gate fires before the audit is even printed — a
    // policy-refused publisher leaves no decision for a human to make.
    let policy = match load_pack_policy(&cwd) {
        Ok(policy) => policy,
        Err(code) => return code,
    };
    if let Err(code) = enforce_publisher_allowed(&policy, &manifest.publisher) {
        return code;
    }

    // The audit runs and prints before anything is vendored, let alone
    // run — nothing executes until a human has seen the inventory.
    let audit = audit_pack(clone_dir.path(), manifest.clone());
    print_report(&audit);
    let tests = run_pack_tests(clone_dir.path()).await;
    if !tests.has_tests {
        println!("\ntests: none shipped");
    } else {
        println!("\ntests: {} case(s), {} failed", tests.total, tests.failed);
        for line in &tests.failures {
            println!("  {line}");
        }
    }
    println!();

    // Executors are code, not declarative YAML — the configurable
    // policy decides whether that needs confirmation (`prompt`, the
    // default), is refused outright (`deny`), or passes (`allow`).
    if let Err(code) = enforce_executor_policy(&policy, &manifest, confirmed_executors, "install") {
        return code;
    }

    let commit = match head_commit(clone_dir.path()).await {
        Ok(commit) => commit,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let resolved_ref = match ref_arg {
        Some(ref_arg) => ref_arg.to_string(),
        None => match current_branch(clone_dir.path()).await {
            Ok(branch) => branch,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
    };

    if let Err(e) = vendor_tree(clone_dir.path(), &dest) {
        eprintln!(
            "error: could not vendor the pack into {}: {e}",
            dest.display()
        );
        return ExitCode::FAILURE;
    }
    let content_hash = match hash_tree(&dest) {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut lock = match load_lock(&cwd) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let key = yunta_core::PackLock::key(&manifest.publisher, &manifest.name);
    lock.packs.insert(
        key,
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
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "installed {}/{} @ {} ({}) -> {}",
        manifest.publisher,
        manifest.name,
        manifest.version,
        &commit[..commit.len().min(12)],
        dest.display()
    );
    ExitCode::SUCCESS
}

pub async fn update(publisher_name: &str, new_ref: &str, confirmed_executors: bool) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some((publisher, name)) = publisher_name.split_once('/') else {
        eprintln!("error: `{publisher_name}` isn't `publisher/name`");
        return ExitCode::FAILURE;
    };

    let mut lock = match load_lock(&cwd) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let key = yunta_core::PackLock::key(publisher, name);
    let Some(entry) = lock.packs.get(&key).cloned() else {
        eprintln!(
            "error: `{publisher_name}` isn't installed — `yunta pack add` it first, not update"
        );
        return ExitCode::FAILURE;
    };

    let clone_dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("error: could not create a temp directory to clone into: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = clone_pack(&entry.source, Some(new_ref), clone_dir.path()).await {
        eprintln!(
            "error: could not clone `{}` at `{new_ref}`: {e}",
            entry.source
        );
        return ExitCode::FAILURE;
    }
    let manifest = match read_manifest(clone_dir.path()) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if manifest.publisher != publisher || manifest.name != name {
        eprintln!(
            "error: `{new_ref}` of `{}` identifies itself as `{}/{}`, not `{publisher_name}` — \
             refusing to update a pack into a different one",
            entry.source, manifest.publisher, manifest.name
        );
        return ExitCode::FAILURE;
    }

    // The same policy gates as `add` — a new ref is where new executor
    // code first appears, and an allow-list narrowed since the install
    // must stop pulling from a publisher it no longer trusts.
    let policy = match load_pack_policy(&cwd) {
        Ok(policy) => policy,
        Err(code) => return code,
    };
    if let Err(code) = enforce_publisher_allowed(&policy, publisher) {
        return code;
    }
    if !manifest.declares.executors.is_empty() {
        // The decision being demanded needs the same evidence `add`
        // shows — nothing executes until a human has seen the
        // inventory — printed only when there's a decision to make.
        print_report(&audit_pack(clone_dir.path(), manifest.clone()));
        println!();
    }
    if let Err(code) = enforce_executor_policy(&policy, &manifest, confirmed_executors, "update") {
        return code;
    }

    let commit = match head_commit(clone_dir.path()).await {
        Ok(commit) => commit,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let dest = vendor_dir(&cwd, publisher, name);
    if let Err(e) = std::fs::remove_dir_all(&dest) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "error: could not clear {} before revendoring: {e}",
                dest.display()
            );
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = vendor_tree(clone_dir.path(), &dest) {
        eprintln!(
            "error: could not vendor the pack into {}: {e}",
            dest.display()
        );
        return ExitCode::FAILURE;
    }
    let content_hash = match hash_tree(&dest) {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    lock.packs.insert(
        key,
        PackLockEntry {
            publisher: publisher.to_string(),
            name: name.to_string(),
            source: entry.source,
            r#ref: new_ref.to_string(),
            commit: commit.clone(),
            content_hash,
        },
    );
    if let Err(e) = save_lock(&cwd, &lock) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "updated {publisher}/{name} -> {new_ref} ({})",
        &commit[..commit.len().min(12)]
    );
    ExitCode::SUCCESS
}

pub fn remove(publisher_name: &str) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some((publisher, name)) = publisher_name.split_once('/') else {
        eprintln!("error: `{publisher_name}` isn't `publisher/name`");
        return ExitCode::FAILURE;
    };

    let mut lock = match load_lock(&cwd) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let key = yunta_core::PackLock::key(publisher, name);
    if lock.packs.remove(&key).is_none() {
        eprintln!("error: `{publisher_name}` isn't installed");
        return ExitCode::FAILURE;
    }

    let dest = vendor_dir(&cwd, publisher, name);
    if let Err(e) = std::fs::remove_dir_all(&dest) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("error: could not remove {}: {e}", dest.display());
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = save_lock(&cwd, &lock) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    println!("removed {publisher_name}");
    ExitCode::SUCCESS
}

pub fn list() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let lock = match load_lock(&cwd) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if lock.packs.is_empty() {
        println!("no packs installed under {}", packs_root(&cwd).display());
        return ExitCode::SUCCESS;
    }

    // Reproducible offline installs mean verifying against the lock:
    // re-hash what's actually vendored on disk and say so when it no
    // longer matches what the lock recorded, rather than just trusting
    // the lock's own numbers back at the user.
    for (key, entry) in &lock.packs {
        let dest = vendor_dir(&cwd, &entry.publisher, &entry.name);
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
    ExitCode::SUCCESS
}
