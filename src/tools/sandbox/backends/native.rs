use tokio::process::Command;

use crate::tools::sandbox::env::apply_sanitized_env;
use crate::tools::sandbox::types::SandboxExecutionRequest;

pub(crate) fn build_native_command(request: &SandboxExecutionRequest<'_>) -> Command {
    let mut cmd = Command::new(request.program);
    cmd.args(request.args).current_dir(request.working_dir);
    if request.sanitize_env {
        apply_sanitized_env(&mut cmd, false, request.extra_env);
    } else {
        for (k, v) in request.extra_env {
            cmd.env(k, v);
        }
    }
    cmd
}
