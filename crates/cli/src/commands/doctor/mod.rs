//! `yunta doctor`: probes every adapter this project's `runners:`
//! names — binary present, version compatible, auth valid — and
//! reports each one, healthy or not. The same `probe()` a real
//! `yunta run`/`yunta resume` calls before spending anything (see
//! `commands::probe_or_refuse`); this command exists to run it on
//! demand and report every result instead of stopping at the first
//! failure.
//!
//! Says who git commits a run's work as, or that it cannot name anyone.
//!
//! Says what the project's checkouts keep of the disk: within their
//! budget, and over the part of it always kept free — under which a run
//! stops.
//!
//! Says, on macOS, what a new binary's first run costs here — what a run
//! whose suite links its test binaries again pays for each of them.
//!
//! Also validates every installed pack's own `requires:` against this
//! project's merged config: roles resolvable, `mcp_servers:` defined,
//! and — the one part `yunta_engine::check_pack_requires` deliberately
//! leaves to this command, since it needs real filesystem access —
//! `requires.programs` present on `PATH`.

mod first_run;
mod packs;
mod session;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use super::verify::{checklist, tallied, CheckRow};
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::json::SCHEMA_VERSION;
use crate::output::{emit, Answer, Mode};
use surco::components::{Check, Checklist, Fix, Found, Furrow, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::port::ProbeReport;
use yunta_core::AdapterId;

pub async fn doctor(
    session: bool,
    full: bool,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let mut checks = Checklist::default();
    let (healthy, answered) = probe_adapters(&ctx, &mut checks).await;
    match super::committer::committer(&ctx).await {
        Ok(who) => checks.push(Found::Holds, "git", format!("commits as {who}")),
        Err(why) => checks.push(Found::Problem, "git", why.to_string()),
    }
    checks.extend(super::forge::forge_checks(&ctx).await?);
    checkouts(&ctx, &mut checks).await;
    checks.extend(first_run::first_run().await);
    // A runner a pack lacks is declared on an adapter that answers here.
    packs::check_requires(&ctx, &answered, &mut checks);
    packs::check_workflows(&ctx, &answered, &mut checks).await?;
    if session {
        probe_sessions(&ctx, &healthy, &mut checks).await;
    }

    emit(output, &DoctorAnswer::of(session, full, &checks), look)
}

/// What `doctor` found, as a document carries it: every check in the order
/// it ran, each with what it means and how to make it hold when the check
/// knows.
#[derive(serde::Serialize)]
pub(crate) struct DoctorAnswer {
    schema_version: u32,
    /// Whether `--session` opened a session per binding.
    session: bool,
    holds: bool,
    checks: Vec<CheckRow>,
    /// Whether the page lists every check on its own row: a glance folds
    /// the ones that hold into one.
    #[serde(skip)]
    full: bool,
}

impl DoctorAnswer {
    /// Whether this machine is ready to run, and how many checks stop a
    /// run or are worth a look.
    fn headline(&self) -> Headline {
        let (mark, said) = match self.holds {
            true => (Mark::Done, "ready"),
            false => (Mark::Failed, "not ready"),
        };
        tallied(
            Headline::new(mark, super::advice::doctor(), said),
            &self.checks,
        )
    }

    fn of(session: bool, full: bool, checks: &Checklist) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            session,
            holds: checks.holds(),
            checks: checks.checks().map(CheckRow::of).collect(),
            full,
        }
    }
}

impl Answer for DoctorAnswer {
    fn document(&self, _look: &Look) -> Doc {
        let checks = match self.full {
            true => checklist(&self.checks),
            false => checklist(&self.checks).folded("healthy"),
        };
        let doctor = super::advice::doctor();
        let mut next = Vec::new();
        if !self.session {
            next.push((
                format!("{doctor} --session"),
                "opens a session per binding, a prompt each",
            ));
        }
        // A glance that folds leaves out what each check that holds found.
        if checks.folds() {
            next.push((
                format!("{doctor} --full"),
                "every check on its own row, with what it found",
            ));
        }
        Doc::new()
            .with(self.headline())
            .with(Furrow(Vec::new()))
            .with(checks)
            .with(Next { steps: next })
    }

    fn outcome(&self) -> Outcome {
        match self.holds {
            true => Outcome::Success,
            false => Outcome::Reported,
        }
    }
}

/// What the project's checkouts keep of the disk: a problem when the disk
/// is already under the part of it kept free — a run stops there — and a
/// caution when their builds are over budget, which the next checkout
/// taken or `yunta gc` empties.
async fn checkouts(ctx: &Context, checks: &mut Checklist) {
    let limits = ctx
        .project
        .config
        .storage
        .as_ref()
        .and_then(|storage| storage.pool.as_ref());
    let pool = yunta_engine::CheckoutPool::upkeep(
        &ctx.project.worktrees_root,
        &ctx.cwd,
        limits,
        std::sync::Arc::new(ctx.clock),
    );
    match pool.report(ctx.supervision()).await {
        Ok(kept) => checks.push_check(kept_check(&kept)),
        Err(e) => checks.push(Found::Caution, "checkouts", e.to_string()),
    }
}

/// What the checkouts keep, as its check: a problem when the disk is
/// already under the part kept free, and a caution when their builds are
/// over their budget, each with what to do about it.
fn kept_check(kept: &yunta_engine::PoolReport) -> Check {
    let check = |found| Check::new(found, "checkouts", kept.to_string());
    let gc = super::advice::gc();
    match (kept.available < kept.keep_free, kept.builds > kept.budget) {
        (true, _) => check(Found::Problem).fix(Fix::said(yunta_render::pause::make_room(&gc))),
        (false, true) => check(Found::Caution)
            .so("the next checkout taken empties the builds past the budget")
            .fix(Fix::said(format!("run `{gc}` to empty them now"))),
        (false, false) => check(Found::Holds),
    }
}

/// Probes every adapter this project's `runners:` names, a check each.
/// Hands back the ones that answered healthy, the only ones a session is
/// worth opening on, and the adapters a runner to declare would run on:
/// those, or every adapter CLI this machine answers for when none of them
/// did.
async fn probe_adapters(
    ctx: &Context,
    checks: &mut Checklist,
) -> (BTreeSet<AdapterId>, Vec<AdapterId>) {
    let adapters = ctx.adapters();
    if adapters.is_empty() {
        let answered = no_runner_declared(ctx, checks).await;
        return (BTreeSet::new(), answered);
    }
    let mut healthy = BTreeSet::new();
    let mut names: Vec<&AdapterId> = adapters.keys().collect();
    names.sort();
    for name in names {
        let Some(adapter) = adapters.get(name) else {
            continue;
        };
        let check = probed(name, adapter.probe().await);
        if check.found == Found::Holds {
            healthy.insert(name.clone());
        }
        checks.push_check(check);
    }
    let answered = match healthy.is_empty() {
        true => crate::detect::healthy(&crate::detect::probe_known_adapters().await),
        false => healthy.iter().cloned().collect(),
    };
    (healthy, answered)
}

/// What probing the adapter `name` answered, as its check.
fn probed(name: &AdapterId, answer: Result<ProbeReport, impl std::fmt::Display>) -> Check {
    match answer {
        Ok(ProbeReport::Healthy { version }) => {
            let said = version
                .as_deref()
                .map(|v| format!(" ({v})"))
                .unwrap_or_default();
            Check::new(Found::Holds, name.as_str(), format!("healthy{said}"))
                .brief(versioned(name, version.as_deref()))
        }
        Ok(ProbeReport::Unhealthy { diagnostic }) => Check::new(
            Found::Problem,
            name.as_str(),
            format!("unhealthy: {diagnostic}"),
        ),
        Err(e) => Check::new(Found::Problem, name.as_str(), format!("unhealthy: {e}")),
    }
}

/// What names a healthy adapter where the checks that hold fold: its id,
/// and the version number its CLI reported, when it reported one.
fn versioned(name: &AdapterId, version: Option<&str>) -> String {
    let number = version.and_then(|version| {
        version
            .split_whitespace()
            .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))
    });
    match number {
        Some(number) => format!("{name} {number}"),
        None => name.to_string(),
    }
}

/// What `doctor` finds of a project whose `runners:` names no adapter
/// this build supports: that nothing has an adapter to run on, and the
/// runner to declare with the adapters this machine answers for, which it
/// hands back. A problem when a workflow in the catalog needs one — it
/// would stop the first time it reached an agent node — and a caution
/// otherwise.
async fn no_runner_declared(ctx: &Context, checks: &mut Checklist) -> Vec<AdapterId> {
    let needing = needing_a_runner(ctx);
    let errors: Vec<yunta_engine::CheckError> = needing
        .iter()
        .flat_map(|(_, errors)| errors.iter().cloned())
        .collect();
    let answered = crate::detect::healthy(&crate::detect::probe_known_adapters().await);
    let detected = crate::detect::Detected {
        adapters: answered.clone(),
        ..crate::detect::Detected::default()
    };
    // The errors are all about runners, so what answers them is the one
    // runner to declare.
    let fix = crate::detect::suggestions(&errors, &detected)
        .pop()
        .unwrap_or_else(|| crate::detect::runner_step(&[], true, &answered));
    let names: Vec<&str> = needing.iter().map(|(name, _)| name.as_str()).collect();
    let check = match names.is_empty() {
        true => Check::new(
            Found::Caution,
            "runners",
            "none declared: a workflow's agent nodes cannot run until one is",
        ),
        false => Check::new(
            Found::Problem,
            "runners",
            format!(
                "none declared, and {} {} one",
                surco::text::listed(names.iter().copied()),
                surco::text::agreeing(names.len(), "needs", "need")
            ),
        ),
    };
    checks.push_check(check.fix(fix));
    answered
}

/// Every catalog workflow that names a runner the config lacks, with
/// what `check` says about each.
fn needing_a_runner(ctx: &Context) -> Vec<(String, Vec<yunta_engine::CheckError>)> {
    let facts = super::declared_facts(&ctx.project.config);
    super::list::catalog_workflows(&ctx.cwd)
        .into_iter()
        .map(|(name, workflow)| {
            let errors = yunta_engine::check(&workflow, &ctx.project.config, &facts)
                .into_iter()
                .filter(names_a_runner)
                .collect();
            (name, errors)
        })
        .filter(|(_, errors): &(String, Vec<_>)| !errors.is_empty())
        .collect()
}

/// Whether `error` is about a runner the config lacks.
fn names_a_runner(error: &yunta_engine::CheckError) -> bool {
    matches!(
        error,
        yunta_engine::CheckError::UnknownRunner { .. }
            | yunta_engine::CheckError::RunnerHasNoCandidates { .. }
            | yunta_engine::CheckError::Unset {
                key: yunta_core::ConfigKey::Runner,
                ..
            }
    )
}

/// Opens one session per binding whose adapter probed healthy, a check
/// each.
///
/// Only the bindings whose adapter is already healthy: the rest have
/// nothing a session could add, and the checks above already name them.
async fn probe_sessions(ctx: &Context, healthy: &BTreeSet<AdapterId>, checks: &mut Checklist) {
    let bindings: Vec<session::Binding> = session::bindings(&ctx.project.config)
        .into_iter()
        .filter(|binding| healthy.contains(&binding.candidate.adapter))
        .collect();
    if bindings.is_empty() {
        checks.push(
            Found::Caution,
            "sessions",
            "no binding to open a session on",
        );
        return;
    }
    for binding in &bindings {
        checks.push_check(session::session_probe(ctx, binding).await.check());
    }
}
