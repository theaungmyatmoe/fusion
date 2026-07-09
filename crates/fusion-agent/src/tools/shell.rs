use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Execute a shell command with permission prompt and timeout.
pub async fn execute(cwd: &str, args: &serde_json::Value) -> Result<String, String> {
    let command = args["command"]
        .as_str()
        .ok_or("run_command: command is required")?;
    let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(30);

    // Permission prompt will be handled by the TUI layer before calling this.
    // This tool just executes — the TUI is responsible for the y/N/a prompt.

    let result = timeout(
        Duration::from_secs(timeout_secs),
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            let mut result_text = String::new();
            if !stdout.is_empty() {
                result_text.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !result_text.is_empty() {
                    result_text.push('\n');
                }
                result_text.push_str("[stderr] ");
                result_text.push_str(&stderr);
            }
            if result_text.is_empty() {
                result_text = format!("(exit code {})", exit_code);
            } else if exit_code != 0 {
                result_text.push_str(&format!("\n(exit code {})", exit_code));
            }

            Ok(result_text)
        }
        Ok(Err(e)) => Err(format!("Command failed: {}", e)),
        Err(_) => Err(format!(
            "Command timed out after {} seconds",
            timeout_secs
        )),
    }
}
