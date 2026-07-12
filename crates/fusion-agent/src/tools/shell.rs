use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::time::timeout;

/// Execute a shell command with permission prompt and timeout.
pub async fn execute(
    cwd: &str,
    main_cwd: Option<&str>,
    args: &serde_json::Value,
) -> Result<String, String> {
    execute_streaming(cwd, main_cwd, args, None).await
}

/// Execute a shell command while forwarding stdout and stderr chunks in real time.
pub async fn execute_streaming(
    cwd: &str,
    main_cwd: Option<&str>,
    args: &serde_json::Value,
    output_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) -> Result<String, String> {
    let command_raw = args["command"]
        .as_str()
        .ok_or("run_command: command is required")?;
    let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(30);

    let command = if let Some(main) = main_cwd {
        command_raw.replace(main, cwd)
    } else {
        command_raw.to_string()
    };

    let mut child = Command::new("sh");
    child
        .arg("-c")
        .arg(&command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = child
        .spawn()
        .map_err(|e| format!("Command failed to start: {}", e))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture command stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture command stderr")?;

    let stdout_task = tokio::spawn(read_stream(stdout, output_tx.clone()));
    let stderr_task = tokio::spawn(read_stream(stderr, output_tx));

    let run = async move {
        let status = child
            .wait()
            .await
            .map_err(|e| format!("Command failed: {}", e))?;
        let stdout = stdout_task
            .await
            .map_err(|e| format!("Failed to read command stdout: {}", e))?
            .map_err(|e| format!("Failed to read command stdout: {}", e))?;
        let stderr = stderr_task
            .await
            .map_err(|e| format!("Failed to read command stderr: {}", e))?
            .map_err(|e| format!("Failed to read command stderr: {}", e))?;

        Ok::<_, String>((status.code().unwrap_or(-1), stdout, stderr))
    };

    let (exit_code, stdout, stderr) = timeout(Duration::from_secs(timeout_secs), run)
        .await
        .map_err(|_| format!("Command timed out after {} seconds", timeout_secs))??;

    let stdout = String::from_utf8_lossy(&stdout);
    let stderr = String::from_utf8_lossy(&stderr);
    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str("[stderr] ");
        result.push_str(&stderr);
    }
    if result.is_empty() {
        result = format!("(exit code {})", exit_code);
    } else if exit_code != 0 {
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&format!("(exit code {})", exit_code));
    }

    Ok(result)
}

async fn read_stream<R>(
    mut reader: R,
    output_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut collected = Vec::new();
    let mut buffer = vec![0_u8; 4096];

    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        collected.extend_from_slice(chunk);
        if let Some(tx) = &output_tx {
            let _ = tx.send(String::from_utf8_lossy(chunk).into_owned());
        }
    }

    Ok(collected)
}

#[cfg(test)]
mod tests {
    use super::execute_streaming;

    #[tokio::test]
    async fn streams_command_output_and_keeps_final_result() {
        let args = serde_json::json!({
            "command": "printf first; sleep 0.01; printf second",
            "timeout_secs": 2
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let result = execute_streaming(".", None, &args, Some(tx))
            .await
            .expect("command should succeed");
        let mut streamed = String::new();
        while let Ok(chunk) = rx.try_recv() {
            streamed.push_str(&chunk);
        }

        assert_eq!(result, "firstsecond");
        assert_eq!(streamed, "firstsecond");
    }
}
