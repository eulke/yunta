//! What a session holds that no reader may be shown, gathered into the one
//! redactor that takes it out of what the session says.

use super::SessionRequest;
use crate::Redactor;

impl SessionRequest {
    /// Takes out of a session's words every secret the session holds: each
    /// declared secret its environment binds, the token of its run tools, and
    /// the token its hooks reach the engine with. The adapter puts the hooks'
    /// token in the environment only as it launches the CLI, so it is read
    /// here from where the request keeps it.
    pub fn redactor(&self) -> Redactor {
        let tokens = self
            .run_tools_endpoint
            .iter()
            .map(|endpoint| endpoint.token.clone())
            .chain(self.rail.iter().map(|rail| rail.token.clone()));
        Redactor::of_values(self.env.secret.values().cloned().chain(tokens))
    }
}
