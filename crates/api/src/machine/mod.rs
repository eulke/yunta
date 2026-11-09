//! What this machine and this binary answer for: the adapters it builds,
//! each under the settings a config gives it, and what a repository says
//! about itself before anyone configures Yunta for it.

mod adapters;
mod detection;

pub use adapters::{
    built_adapter, built_adapter_ids, built_adapter_names, built_adapters, declared_facts,
    first_built_adapter, real_adapters, service_vars, Adapters,
};
pub use detection::{adapters_for, fix_for, in_files, in_repo, probe_known_adapters, suggestions};
