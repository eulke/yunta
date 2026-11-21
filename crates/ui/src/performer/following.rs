//! See [`super`]. The performer's one following. The first [`Request::Follow`] makes it
//! with `Project::follow`, on a task of its own; each later one goes to that task, which
//! follows it anew with `refollow` between two looks. Each piece of news goes to the
//! inputs as [`Input::FollowNews`], under no ticket: a following is never let go, only
//! followed anew.
//!
//! A request handed with other inputs than the ones the following hands its news to makes
//! it anew for them, and the one under way ends: whoever reads those inputs takes what is
//! followed whole, as a state handed to a new reader of its inputs needs, where news handed
//! on to the old ones would be lost.
//!
//! It never ends by itself. The performer holds the one end of what the task is asked
//! through, so dropping the performer ends the task, and the reader it looks through
//! with it. A look that fails whole, the log unreadable, is told as
//! [`Input::LookFailed`] and the following goes on: `Following::next` spaces each look
//! from the one before, so a log that stays unreadable is told once a look, at the
//! follower's own pace. Making the following is its first look: a try that fails is told
//! the same way, and the following is made again a pace after that try began, at
//! [`Following::LOOKS_EVERY`], with the newest [`Followed`] asked by then, until it is
//! made or the performer is dropped.
//!
//! [`Request::Follow`]: crate::Request::Follow

use std::sync::{Mutex, PoisonError};

use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tokio::time::Instant;
use yunta_api::{Error, Followed, Following, Stop, Workspace};
use yunta_core::pace::Pace;

use crate::Input;

/// The task the performer's one following runs on, as the performer holds it: what it
/// hands that task each [`Followed`] through. Not `yunta-api`'s `Following`, which the
/// task holds.
#[derive(Default)]
pub(super) struct FollowingTask {
    /// The end the task reads what to follow from, closed once the task ended, and the
    /// inputs the task hands its news to: none before the first request.
    asked: Mutex<Option<(watch::Sender<Followed>, UnboundedSender<Input>)>>,
}

impl FollowingTask {
    /// `followed` handed to the following under way where it hands its news to `inputs`;
    /// else followed by one made in `workspace`'s project on a task of its own, its news
    /// handed to `inputs`, and the one under way ended.
    pub(super) fn follow(
        &self,
        workspace: &Workspace,
        followed: Followed,
        inputs: &UnboundedSender<Input>,
    ) {
        let mut asked = self.asked.lock().unwrap_or_else(PoisonError::into_inner);
        let under_way = asked
            .as_ref()
            .filter(|(asking, handed)| !asking.is_closed() && handed.same_channel(inputs));
        if let Some((asking, _)) = under_way {
            asking.send_replace(followed);
            return;
        }
        // The sender replaced drops, which ends the task that read from it.
        let (asking, told) = watch::channel(followed);
        *asked = Some((asking, inputs.clone()));
        tokio::spawn(following(workspace.clone(), told, inputs.clone()));
    }
}

/// What `told` names, followed in `workspace`'s project and handed to `inputs`, each
/// [`Followed`] it is told after the first followed anew, and each look that failed told:
/// until `told`'s sender drops, nobody reads `inputs`, or a look is stopped.
async fn following(
    workspace: Workspace,
    mut told: watch::Receiver<Followed>,
    inputs: UnboundedSender<Input>,
) {
    let Some(mut following) = made(&workspace, &mut told, &inputs).await else {
        return;
    };
    // What was told while it was made is followed from its first look.
    following.refollow(told.borrow_and_update().clone());
    // Nothing trips it: dropping the following stops what it runs.
    let stop = Stop::new();
    loop {
        // What to follow is taken before the next news, so nothing more of what was let
        // go is looked for once something else is asked.
        tokio::select! {
            biased;
            changed = told.changed() => match changed {
                Ok(()) => following.refollow(told.borrow_and_update().clone()),
                Err(_) => return,
            },
            news = following.next(&stop) => {
                // A look that failed is told and followed past: the next comes a pace
                // after it, as `next` spaces every look from the one before.
                let input = match news {
                    Ok(news) => Some(Input::FollowNews(news)),
                    Err(failed) => look_failed(failed),
                };
                let Some(input) = input else {
                    return;
                };
                if inputs.send(input).is_err() {
                    return;
                }
            }
        }
    }
}

/// The following of the newest [`Followed`] `told` names, made in `workspace`'s
/// project. Making it is its first look: a try that fails is told to `inputs` as one, and
/// the next try begins a pace after it began, with what `told` names by then. None once
/// `told`'s sender drops, nobody reads `inputs`, or a try is stopped.
async fn made(
    workspace: &Workspace,
    told: &mut watch::Receiver<Followed>,
    inputs: &UnboundedSender<Input>,
) -> Option<Following> {
    loop {
        let began = Instant::now();
        let first = told.borrow_and_update().clone();
        let mut making = std::pin::pin!(async {
            let project = workspace.project()?;
            project.follow(first).await
        });
        let failed = loop {
            tokio::select! {
                made = &mut making => match made {
                    Ok(following) => return Some(following),
                    Err(failed) => break failed,
                },
                changed = told.changed() => changed.ok()?,
            }
        };
        inputs.send(look_failed(failed)?).ok()?;
        let mut paced = std::pin::pin!(Pace::every(Following::LOOKS_EVERY).beat_after(began));
        loop {
            tokio::select! {
                () = &mut paced => break,
                changed = told.changed() => changed.ok()?,
            }
        }
    }
}

/// What a look that failed with `failed` hands the inputs, in the words a failed piece of
/// work is said in: none where it was stopped, which ends the following.
fn look_failed(failed: Error) -> Option<Input> {
    match failed {
        Error::Stopped => None,
        failed => Some(Input::LookFailed(failed.to_string())),
    }
}
