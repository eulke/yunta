//! What a run spent and where its time went, as `yunta stats <run>` says it
//! and as a run's screen shows it, in less ([`Stats`], [`StatsTold`]), and what a
//! workflow's past runs say, as `yunta stats --workflow` does ([`History`]), with
//! what its history says of how effective its checks were.

mod facts;
mod history;
mod run;
mod time;

pub use history::History;
pub use run::{Stats, StatsTold};
