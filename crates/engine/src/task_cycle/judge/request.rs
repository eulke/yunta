//! How the request a session made is decided: by the evidence a red criterion
//! of its task locates, else by its proposed criterion. With no request, the
//! decision is on the refused writes a red criterion locates. Every decision
//! is settled through `scope_expansion::expand` against the window the
//! judgement counts.

use std::path::Path;

use yunta_core::events::ToolTarget;
use yunta_core::expansion::{Ask, Decision, Precheck, Proposal};
use yunta_core::ScopeGlob;

use super::{Deciding, Refusals, Settle, Sitting};
use crate::scope_expansion::{
    locates, relative, standing_refusal, GrantWindow, ScopeExpansionOutcome, ScopeExpansionRequest,
};
use crate::task_cycle::evidence::{
    first_located, is_red, located, locates_every, precheck_of, text_of, Kept, Located,
};
use crate::task_cycle::{could_not_run, CriterionRun, TaskCycleError};

/// The decision on the request the session made, or, with no request, on the
/// refused writes a red criterion locates. A node decides no request: its own
/// menu answers the one it makes. Nothing to decide leaves the window untouched.
pub(super) async fn decide_request(
    sitting: &Sitting<'_>,
    runs: &[CriterionRun],
    refused: &[ToolTarget],
    settle: Settle,
) -> Result<Option<ScopeExpansionOutcome>, TaskCycleError> {
    let Some(task) = sitting.task else {
        return Ok(None);
    };
    if sitting.request.is_none() && refused.is_empty() {
        return Ok(None);
    }
    let (Some(window), Some(bounds)) = (sitting.window, sitting.bounds) else {
        return Ok(None);
    };
    let seeded;
    let window = match settle {
        Settle::Dry => {
            seeded = GrantWindow::seeded(window.granted().await);
            &seeded
        }
        Settle::Commit => window,
    };
    let deciding = Deciding {
        task,
        env: &sitting.env,
        checkout: sitting.work.checkout,
        kept: match &sitting.refused {
            Refusals::Observer { observer, .. } => Kept::Observer(*observer),
            Refusals::Log { run_dir, .. } => Kept::Run(run_dir),
            Refusals::None => Kept::Nowhere,
        },
        window,
        bounds,
    };
    let red = runs.iter().any(is_red);
    match sitting.request {
        Some(request) => decide_asked(&deciding, request, runs, red).await.map(Some),
        None if !red => Ok(None),
        None => match of_refusals(&deciding, refused, runs).await {
            Some(located) => deciding.decide_located(located).await.map(Some),
            None => Ok(None),
        },
    }
}

/// The outcome of the request a session made: decided by its evidence when a
/// red criterion of the task locates every path it asks for, else by its
/// proposed criterion. A refusal the protection or the mode settles is given
/// before any criterion runs, so it runs none. Any other request runs its
/// criterion, and the pre-check that run reads decides it.
async fn decide_asked(
    deciding: &Deciding<'_>,
    request: &ScopeExpansionRequest,
    runs: &[CriterionRun],
    red: bool,
) -> Result<ScopeExpansionOutcome, TaskCycleError> {
    if red {
        if let Some(located) = of_evidence(deciding, request).await? {
            return deciding.decide_located(located).await;
        }
    }
    if let Some(refused) = refused_before_running(deciding, request).await {
        return Ok(refused);
    }
    // The criterion runs, and a passing one is denied by its real pre-check,
    // which the screen above does not read.
    let proposed = request.proposed_criterion.as_ref();
    let precheck = match proposed {
        Some(criterion) => Some(precheck_of(deciding, &criterion.cmd).await?),
        None => None,
    };
    let proposal = proposed
        .zip(precheck)
        .map(|(criterion, precheck)| Proposal {
            cmd: &criterion.cmd,
            precheck,
        });
    let (decision, count) = deciding
        .decide(Ask::Requested {
            paths: &request.paths,
            reason: &request.reason,
            proposal,
        })
        .await?;
    let outcome = ScopeExpansionOutcome {
        asked: Some(request.clone()),
        precheck,
        decision,
        count,
    };
    if !red || standing(&outcome.decision) {
        return Ok(outcome);
    }
    match of_red(deciding, &outcome, runs).await {
        Some(located) => deciding.decide_located(located).await,
        None => Ok(outcome),
    }
}

/// The outcome of a refusal that stands, which the protection or the mode
/// settles on the request as it reads with its criterion left unrun: the
/// refusal the call that asked gives too. `None` when no such refusal settles
/// it. Any other answer is one a real pre-check, or a red output, may still
/// change, so the criterion runs.
async fn refused_before_running(
    deciding: &Deciding<'_>,
    request: &ScopeExpansionRequest,
) -> Option<ScopeExpansionOutcome> {
    let denial = standing_refusal(request, &deciding.bounds)?;
    Some(ScopeExpansionOutcome {
        asked: Some(request.clone()),
        precheck: request
            .proposed_criterion
            .as_ref()
            .map(|_| Precheck::NotRun),
        decision: Decision::Deny(denial),
        count: deciding.window.granted().await,
    })
}

/// Whether a decision stands without a red output's say: a grant, or a refusal
/// no work changes.
fn standing(decision: &Decision) -> bool {
    match decision {
        Decision::Grant { .. } => true,
        Decision::Deny(denial) => denial.stands(),
        Decision::Escalate { .. } => false,
    }
}

/// The facts of `request` when it cites one of the task's criteria as its
/// evidence, and that criterion, run on the work, is red and locates every path
/// the request asks for.
async fn of_evidence(
    deciding: &Deciding<'_>,
    request: &ScopeExpansionRequest,
) -> Result<Option<Located>, TaskCycleError> {
    let cited = request
        .evidence
        .as_deref()
        .filter(|cmd| deciding.task.criteria.iter().any(|c| c.cmd == *cmd));
    let Some(cmd) = cited else {
        return Ok(None);
    };
    let answer = deciding
        .env
        .memo
        .exit_code(cmd, deciding.checkout, deciding.env.supervision)
        .await?;
    let red = answer.exit_code != 0 && could_not_run(answer.exit_code).is_none();
    let Some(printed) = answer.output.as_ref() else {
        return Ok(None);
    };
    let text = text_of(&deciding.kept, deciding.checkout, printed).await;
    if !red || !locates_every(&text, &request.paths) {
        return Ok(None);
    }
    Ok(located(
        request.paths.clone(),
        cmd.to_string(),
        Some(request.clone()),
        None,
    ))
}

/// The first red output that locates one of the refused writes, with the writes
/// it locates.
async fn of_refusals(
    deciding: &Deciding<'_>,
    refused: &[ToolTarget],
    runs: &[CriterionRun],
) -> Option<Located> {
    let refused: Vec<String> = refused
        .iter()
        .filter_map(|target| relative(deciding.checkout, target.display()?))
        .collect();
    first_located(deciding, runs, |text, cmd| {
        let here: Vec<&String> = refused.iter().filter(|path| locates(text, path)).collect();
        let paths = here
            .into_iter()
            .filter_map(|path| ScopeGlob::exact(Path::new(path)).ok())
            .collect();
        located(paths, cmd.to_string(), None, None)
    })
    .await
}

/// The facts of a request that a red output of this judgement locates in full,
/// keeping the request's own pre-check. `None` when no red output does.
async fn of_red(
    deciding: &Deciding<'_>,
    outcome: &ScopeExpansionOutcome,
    runs: &[CriterionRun],
) -> Option<Located> {
    let request = outcome.asked.as_ref()?;
    first_located(deciding, runs, |text, cmd| {
        if !locates_every(text, &request.paths) {
            return None;
        }
        located(
            request.paths.clone(),
            cmd.to_string(),
            Some(request.clone()),
            outcome.precheck,
        )
    })
    .await
}
