//! `yunta mcp`: the control plane, `yunta-mcp`'s [`ControlPlane`], served
//! on stdio over the workspace this process read. A SIGINT stops what a tool
//! is doing and closes the server: nothing is left half-answered on stdout.
//! What the API says while a tool works goes to stderr, quietly, since
//! stdout is the stream the replies travel on.

use std::sync::Arc;

use rmcp::ServiceExt;
use yunta_api::Remark;
use yunta_mcp::ControlPlane;

use crate::error::CliError;
use crate::output::{Mode, Outcome};
use crate::shell::Shell;

pub async fn mcp(shell: &Shell) -> Result<Outcome, CliError> {
    let stop = shell.stop();
    // Quiet, because stdout is the JSON-RPC stream: what a person would read
    // as a progress line would land in the middle of a reply; a caution
    // still goes to stderr. Quiet draws nothing, so the look only completes
    // the call.
    let look = crate::output::stdout_look();
    let remarks = Arc::new(move |remark: Remark| crate::remarks::say(remark, Mode::Quiet, &look));
    let plane = ControlPlane::new(shell.workspace().clone(), stop.clone(), remarks);
    let server = plane
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| CliError::msg(format!("cannot start the MCP server: {e}")))?;
    tokio::select! {
        ended = server.waiting() => {
            ended.map_err(|e| CliError::msg(format!("MCP server exited abnormally: {e}")))?;
            Ok(Outcome::Success)
        }
        // The interrupt closes the server the way the process exiting
        // would: nothing is left half-answered on stdout, and the code says
        // a person stopped it.
        () = stop.work().cancelled() => Ok(Outcome::interrupted()),
    }
}
