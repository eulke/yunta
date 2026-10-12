//! How the mock plans the launch of the CLI its fixture stands for. The planner
//! is the one the real adapter calls. The one difference is the hook a session
//! that judges no write through a hook is planned under: it is never run.

use std::path::PathBuf;

use yunta_core::port::SessionRequest;
use yunta_core::rail::{RailCommand, RailEndpoint};
use yunta_core::{Result, Secret, SessionId};

use super::MockAdapter;
use crate::dialect::Plan;

impl MockAdapter {
    /// Plans the launch of this mock's CLI for `req`. A session whose writes
    /// the hook judges needs its hook, and the planner refuses without one. A
    /// session that judges no write through a hook never asks the engine, so
    /// it is planned under a hook that is never run.
    pub(super) fn plan(&self, req: &SessionRequest, resume: Option<&SessionId>) -> Result<Plan> {
        let dialect = self.fixture.dialect();
        if self.asks_per_write() {
            return dialect.plan(&self.settings, req, resume);
        }
        let mut planned = req.clone();
        planned
            .rail_command
            .get_or_insert_with(|| RailCommand::new(PathBuf::from("yunta")));
        planned.rail.get_or_insert_with(|| RailEndpoint {
            // Nothing listens on the discard port, and this session never calls it.
            url: "http://127.0.0.1:9/rail".to_string(),
            token: Secret::new("unreached".to_string()),
        });
        dialect.plan(&self.settings, &planned, resume)
    }
}
