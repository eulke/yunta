//! What a test finds of each of a list of names, taken on every core at once, each name on
//! a deep stack and a runtime of its own: every difference found, in the order of the
//! names, a name whose work panicked named with what the panic said, so no failure hides
//! another.

use std::any::Any;
use std::future::Future;
use std::num::NonZeroUsize;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

/// What `found_in` finds of each of `names`, each taken by one of as many threads as the
/// machine has cores, every one of them in the order of `names`.
pub fn differences<F, Fut>(names: &'static [&'static str], found_in: F) -> Vec<String>
where
    F: Fn(&'static str) -> Fut + Copy + Send + 'static,
    Fut: Future<Output = Vec<String>>,
{
    let cores = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let next = &AtomicUsize::new(0);
    let mut found: Vec<(usize, Vec<String>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..cores.min(names.len()))
            .map(|_| {
                scope.spawn(move || {
                    let mut found = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(name) = names.get(at) else {
                            return found;
                        };
                        found.push((at, differences_of(name, found_in)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_else(|panic| resume_unwind(panic)))
            .collect()
    });
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().flat_map(|(_, found)| found).collect()
}

/// What `found_in` finds of `name`, taken on a deep stack of its own; where it panicked,
/// `name` and what the panic said.
fn differences_of<F, Fut>(name: &'static str, found_in: F) -> Vec<String>
where
    F: Fn(&'static str) -> Fut + Send + 'static,
    Fut: Future<Output = Vec<String>>,
{
    let (found, received) = mpsc::channel();
    let taken = catch_unwind(AssertUnwindSafe(|| {
        yunta_testkit::on_a_deep_stack(move || async move {
            let differences = found_in(name).await;
            found
                .send(differences)
                .expect("the worker waits for what is found");
        })
    }));
    match taken {
        Ok(()) => received.recv().expect("what was found, sent"),
        Err(panic) => vec![format!("{name} panicked: {}", said(&*panic))],
    }
}

/// What a panic said, where it said it in words.
fn said(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("what it said is not words")
}
