//! What an invocation holds over the terminal while its run is drawn,
//! and how it takes it down.

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::CliError;
use crate::output::Mode;
use crate::surface::{Curtain, Diagnostics, EngineReading, Surface, SurfaceEnv};

use super::{Driving, Presentation};

/// Everything this invocation holds while the run is being drawn: the
/// surface the progress goes on, the observer the engine feeds it
/// through, the console the run puts its questions to, and the token a
/// person stops it with.
///
/// One object because they are one arrangement over one terminal — the
/// prompt takes its turn with the surface through the curtain, the
/// interrupt says so above the region through the surface's own door,
/// and a run with no surface has none of it.
pub(super) struct Watching {
    pub(super) surface: Option<Surface>,
    pub(super) observer: Option<std::sync::Arc<dyn yunta_core::port::RunObserver>>,
    pub(super) asking: crate::human_interaction::ConsoleInteraction,
    /// The invocation's first stage, which this run answers to.
    pub(super) cancel: CancellationToken,
    /// The task that tells the person their interrupt arrived. Kept so
    /// it dies with the arrangement it belongs to.
    announcing: Option<JoinHandle<()>>,
}

impl Watching {
    /// Takes the arrangement down: the surface draws everything the
    /// engine handed it and lets the terminal go, and what announced
    /// the interrupt goes with it.
    pub(super) async fn close(mut self) {
        if let Some(surface) = self.surface.take() {
            surface.close().await;
        }
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        if let Some(announcing) = self.announcing.take() {
            announcing.abort();
        }
    }
}

/// What this invocation draws on, asks on, and is stopped through.
///
/// `--json` gets no surface at all: the document is the whole story, so
/// the engine carries no observer, and a prompt has no region to take a
/// turn with.
///
/// The order is the arrangement's own: what announces the interrupt is
/// armed once there is a surface for it to say so through, because the
/// line it raises belongs above the region rather than around it.
pub(super) async fn watch(env: &Driving<'_>, shown: &Presentation) -> Result<Watching, CliError> {
    let surface = if env.output == Mode::Json {
        None
    } else {
        // The run's creation is written before any observer exists, so the
        // first thing the surface knows is read once, here.
        let log = env.ctx.project.event_log().await?;
        let seed = log.events(&env.prepared.run_id).await?;
        Some(Surface::open(
            SurfaceEnv {
                run_id: &env.prepared.run_id,
                manifest: env.manifest,
                prior: env.prior.as_ref(),
                log: &log,
                clock: std::sync::Arc::clone(&env.ctx.clock),
                delivery: shown.delivery,
                glyphs: shown.glyphs,
                engine: EngineReading::This,
            },
            seed,
        ))
    };
    // The token is the invocation's, installed when its `Context`
    // loaded; what this arranges is the line the person reads when they
    // press Ctrl-C, which belongs above the region the surface pins.
    let cancel = env.ctx.cancellation().clone();
    Ok(Watching {
        observer: surface.as_ref().and_then(Surface::observer),
        asking: crate::human_interaction::ConsoleInteraction::new(
            surface
                .as_ref()
                .map_or_else(Curtain::none, Surface::curtain),
            surface
                .as_ref()
                .map_or_else(Diagnostics::none, Surface::diagnostics),
            cancel.clone(),
            env.ctx.env.clone(),
            surface.as_ref().is_some_and(Surface::chronicles),
        ),
        announcing: Some(announce(
            surface
                .as_ref()
                .map_or_else(Diagnostics::none, Surface::diagnostics),
            cancel.clone(),
        )),
        cancel,
        surface,
    })
}

/// Says the interrupt arrived, once, through the surface's own door.
///
/// What it tells the user goes out through `diagnostics`, which is that
/// door: the interrupt arrives while the run is being drawn, and a line
/// printed around the pinned region lands inside the rows it is
/// redrawing — so the person who pressed Ctrl-C would read their
/// confirmation until the next redraw erased it.
fn announce(diagnostics: Diagnostics, cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        cancel.cancelled().await;
        diagnostics
            .raise("interrupt received: stopping the run (sessions get interrupt, then kill)")
            .await;
    })
}
