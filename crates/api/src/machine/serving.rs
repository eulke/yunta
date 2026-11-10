//! See [`super`]. What this binary refuses to serve for a run, before the run
//! exists or a resume takes anything. It judges only what the run reaches —
//! the nodes its mode includes, and each frozen `use:` child one of them runs
//! — and probes only the adapters those nodes' sessions resolve to, so a run,
//! a resume and every interface refuse the same run in the same words.

use std::collections::HashSet;
use std::path::Path;

use yunta_core::port::ProbeReport;
use yunta_core::refusal::Unservable;
use yunta_core::step::Arg;
use yunta_core::{
    AdapterId, ConfigLayer, Manifest, ModeName, MountSpec, Node, NodeKind, RunAdapter, Workflow,
};
use yunta_render::refusal::Refused;

use super::Adapters;
use crate::Error;

/// Refuses what this binary cannot serve for a run of `manifest` in `mode`,
/// naming `workflow` when a check refuses it. A mock run's sessions are
/// scripted: no real adapter is built, named or probed for it.
pub async fn servable(
    manifest: &Manifest,
    mode: &ModeName,
    workflow: Arg<String>,
) -> Result<(), Error> {
    let pin = manifest.adapter.as_ref().and_then(RunAdapter::pinned);
    let children = frozen_children(manifest)?;
    let reach = reach(manifest, mode, &children);
    refuse_unlisted_pins(manifest, &reach.nodes, pin)?;
    if matches!(manifest.adapter, Some(RunAdapter::Mock { .. })) {
        return Ok(());
    }
    let adapters = super::real_adapters(&manifest.config);
    if let Some(pin) = pin.filter(|pin| !adapters.contains_key(*pin)) {
        let mut runnable: Vec<AdapterId> = adapters.keys().cloned().collect();
        runnable.sort();
        return Err(refused(Unservable::UnbuiltPin {
            pin: pin.clone(),
            runnable,
        }));
    }
    refuse_sessionless(&reach.nodes, &adapters)?;
    refuse_unchecked(manifest, &reach, workflow)?;
    refuse_unhealthy(&reached_adapters(manifest, &reach.nodes, pin, adapters)).await
}

/// What a refused serving is, as the API hands it on.
fn refused(unservable: Unservable) -> Error {
    Refused::Unservable(unservable).into()
}

/// Refuses the first node that takes a session on a runner with no candidate
/// on the pinned adapter, naming the node, its runner and the candidates.
fn refuse_unlisted_pins(
    manifest: &Manifest,
    nodes: &[&Node],
    pin: Option<&AdapterId>,
) -> Result<(), Error> {
    let Some(pin) = pin else {
        return Ok(());
    };
    for node in nodes {
        // Only a node that takes a session resolves a runner; no other node
        // reaches one, whatever `defaults.runner` names.
        if !node.opens_session() {
            continue;
        }
        let Some(role) = manifest.config.runner_role(node) else {
            continue;
        };
        let candidates = manifest
            .config
            .runners
            .as_ref()
            .and_then(|runners| runners.get(role));
        if !candidates.into_iter().flatten().any(|c| c.adapter == *pin) {
            return Err(refused(Unservable::NoCandidateOnPin {
                node: node.id.clone(),
                runner: role.clone(),
                pin: pin.clone(),
                candidates: candidates
                    .into_iter()
                    .flatten()
                    .map(|candidate| candidate.adapter.clone())
                    .collect(),
            }));
        }
    }
    Ok(())
}

/// Refuses early when one of `nodes` needs an agent session and no available
/// adapter can provide one: an error in check, never emulation at runtime.
pub(crate) fn refuse_sessionless(nodes: &[&Node], adapters: &Adapters) -> Result<(), Error> {
    let needs_sessions = nodes.iter().any(|node| node.opens_session());
    if needs_sessions && adapters.is_empty() {
        return Err(refused(Unservable::NoAdapter {
            built: super::built_adapter_ids(),
        }));
    }
    Ok(())
}

/// Health-checks every adapter a run would use — the binary there and
/// answering `--version`, every setting read — and refuses before any
/// checkout or token is spent if one comes back unhealthy. `doctor` probes
/// the same adapters itself, reporting each as a check of its own.
async fn refuse_unhealthy(adapters: &Adapters) -> Result<(), Error> {
    let mut unhealthy = Vec::new();
    for (name, adapter) in adapters {
        match adapter.probe().await {
            Ok(ProbeReport::Healthy { .. }) => {}
            // An adapter that reports itself unhealthy without saying why
            // is listed by name alone.
            Ok(ProbeReport::Unhealthy { diagnostic }) => {
                unhealthy.push((name.to_string(), diagnostic));
            }
            Err(e) => unhealthy.push((name.to_string(), e.to_string())),
        }
    }
    match unhealthy.is_empty() {
        true => Ok(()),
        false => Err(refused(Unservable::Unhealthy {
            adapters: unhealthy,
        })),
    }
}

/// Each frozen child, read as its parent froze it, under the name its `use:`
/// gives it. The name is the path the child is read by.
fn frozen_children(manifest: &Manifest) -> Result<Vec<(&str, Workflow)>, Error> {
    manifest
        .frozen
        .iter()
        .flat_map(|frozen| &frozen.workflows)
        .map(|(name, child)| {
            let workflow = yunta_core::workflow::read::read(&child.text, Path::new(name)).map_err(
                |report| Refused::Document {
                    report: Box::new(report),
                    text: Some(child.text.clone()),
                },
            )?;
            Ok((name.as_str(), workflow))
        })
        .collect()
}

/// What a run reaches: the nodes its mode includes, `parallel` children
/// included, and each frozen `use:` child one of those nodes runs, with the
/// nodes of the child's own first mode.
struct Reach<'a> {
    nodes: Vec<&'a Node>,
    children: Vec<&'a (&'a str, Workflow)>,
}

/// What a run in `mode` reaches. A child that only a node outside the mode
/// runs is never reached, so nothing of it is judged.
fn reach<'a>(
    manifest: &'a Manifest,
    mode: &ModeName,
    children: &'a [(&'a str, Workflow)],
) -> Reach<'a> {
    let mut reach = Reach {
        nodes: in_mode(&manifest.workflow, mode, &manifest.config),
        children: Vec::new(),
    };
    while let Some(child) = children.iter().find(|(name, _)| {
        !reach.children.iter().any(|seen| seen.0 == *name) && uses(&reach.nodes, name)
    }) {
        reach
            .nodes
            .extend(in_mode(&child.1, &child.1.first_mode(), &manifest.config));
        reach.children.push(child);
    }
    reach
}

/// The nodes `workflow` reaches in `mode`, less what `config` leaves out.
fn in_mode<'a>(workflow: &'a Workflow, mode: &ModeName, config: &ConfigLayer) -> Vec<&'a Node> {
    yunta_replay::modes::reached(workflow, mode, &yunta_core::left_out(workflow, config))
}

/// Whether one of `nodes` runs the frozen child its `use:` calls `name`.
fn uses(nodes: &[&Node], name: &str) -> bool {
    nodes.iter().any(
        |node| matches!(&node.kind, NodeKind::Workflow { r#use, .. } if r#use.as_str() == name),
    )
}

/// The adapters the sessions of `nodes` resolve to, as runner resolution
/// picks them at the node: the pinned one, or a runner's first candidate this
/// binary builds. A node that opens no session reaches none.
fn reached_adapters(
    manifest: &Manifest,
    nodes: &[&Node],
    pin: Option<&AdapterId>,
    mut adapters: Adapters,
) -> Adapters {
    let built = |adapter: &AdapterId| adapters.contains_key(adapter);
    let reached: HashSet<AdapterId> = nodes
        .iter()
        .filter(|node| node.opens_session())
        .filter_map(|node| manifest.config.runner_role(node))
        .filter_map(|role| yunta_engine::resolve_runner(role, &manifest.config, &built, pin).ok())
        .map(|resolved| resolved.chosen.adapter)
        .collect();
    adapters.retain(|id, _| reached.contains(id));
    adapters
}

/// What `check` says of the root and of each frozen child the run reaches,
/// judged against the adapters this binary builds: the capability rules the
/// run's declarations must still meet. A child is judged with the mounts its
/// `use:` nodes carry in, as its own birth judges it.
fn refuse_unchecked(
    manifest: &Manifest,
    reach: &Reach<'_>,
    workflow: Arg<String>,
) -> Result<(), Error> {
    let declared = super::declared_facts(&manifest.config);
    let mut errors = yunta_engine::check(&manifest.workflow, &manifest.config, &declared);
    for (name, child) in &reach.children {
        let mounts = mounts_into(name, &reach.nodes);
        errors.extend(yunta_engine::check_mounted(
            child,
            &manifest.config,
            &declared,
            &mounts,
        ));
    }
    match errors.is_empty() {
        true => Ok(()),
        false => Err(Refused::Unchecked { workflow, errors }.into()),
    }
}

/// Every mount the `use:` nodes of `name` carry in, across the nodes the run
/// reaches. A child used at several places is judged with the mounts of all
/// of them, so a read one use answers is not refused for another's sake.
fn mounts_into(name: &str, nodes: &[&Node]) -> Vec<MountSpec> {
    nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::Workflow { r#use, mounts, .. } if r#use.as_str() == name => Some(mounts),
            _ => None,
        })
        .flatten()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use yunta_core::{describe, AdapterSettings};

    use super::*;

    /// Every sentence that tells a person which adapters exist reads from the
    /// one place they are declared, so a third adapter lands with the
    /// refusal, the probe listing and the `init` template naming it alike.
    #[test]
    fn what_a_refusal_names_is_what_this_binary_builds() {
        let built = super::super::built_adapters(|_| AdapterSettings::default());
        assert!(
            !built.is_empty(),
            "a binary with no adapter can run no session"
        );
        let workflow = yunta_core::yaml::parse::<Workflow>(
            "name: w\nnodes:\n  - id: a\n    kind: prompt\n    runner: r\n    prompt: p\n",
        )
        .expect("the workflow parses");
        let nodes: Vec<&Node> = workflow.iter_nodes().collect();
        let refusal = refuse_sessionless(&nodes, &Adapters::new())
            .expect_err("a prompt node with no adapter is unrunnable");
        let text = describe(&refusal);
        for adapter in &built {
            assert!(
                text.contains(adapter.id().as_str()),
                "the refusal names `{}`, which this binary builds: {text}",
                adapter.id()
            );
        }
    }

    /// An unbuilt pin offers the adapters `runners:` names that this binary
    /// builds, and says so in words of its own when there are none.
    #[test]
    fn an_unbuilt_pin_names_what_this_binary_can_run() {
        let said = |runnable: Vec<AdapterId>| {
            describe(&refused(Unservable::UnbuiltPin {
                pin: AdapterId::from("codex"),
                runnable,
            }))
        };
        assert_eq!(
            said(vec![AdapterId::from("claude-code")]),
            "unknown adapter `codex`; this binary can run: claude-code"
        );
        assert_eq!(
            said(Vec::new()),
            "unknown adapter `codex`; this binary can run: (none configured: `runners:` \
             names no adapter this build supports)"
        );
    }
}
