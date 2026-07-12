use serde::Deserialize;
use std::process::Stdio;
use std::time::Duration;

use fusion_core::config::{fusion_temp_dir, is_termux, remap_tmp_paths};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Deserialize)]
struct RunCommandArgs {
    command: String,
    timeout_secs: Option<u64>,
    head_lines: Option<usize>,
    tail_lines: Option<usize>,
    _reason: Option<String>,
}

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
    let args: RunCommandArgs = serde_json::from_value(args.clone())
        .map_err(|e| format!("Invalid run_command arguments: {}", e))?;

    let timeout_secs = args.timeout_secs.unwrap_or(30);

    let mut command = if let Some(main) = main_cwd {
        args.command.replace(main, cwd)
    } else {
        args.command.clone()
    };

    // Termux (and similar): remap /tmp → writable fusion temp dir and export TMPDIR.
    let tmp_dir = fusion_temp_dir();
    let tmp_str = tmp_dir.to_string_lossy().to_string();
    if is_termux() || command.contains("/tmp") {
        command = remap_tmp_paths(&command, &tmp_dir);
    }

    let mut child = Command::new("sh");
    child
        .arg("-c")
        .arg(&command)
        .current_dir(cwd)
        .env("TMPDIR", &tmp_str)
        .env("TMP", &tmp_str)
        .env("TEMP", &tmp_str)
        .env("PAGER", "cat")
        .env("EDITOR", "true")
        .env("GIT_EDITOR", "true")
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

    let (exit_code, stdout_bytes, stderr_bytes) = timeout(Duration::from_secs(timeout_secs), run)
        .await
        .map_err(|_| format!("Command timed out after {} seconds", timeout_secs))??;

    let stdout_raw = String::from_utf8_lossy(&stdout_bytes);
    let stdout = slice_output(&stdout_raw, args.head_lines, args.tail_lines);

    let stderr = String::from_utf8_lossy(&stderr_bytes);
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

fn slice_output(output: &str, head_lines: Option<usize>, tail_lines: Option<usize>) -> String {
    if head_lines.is_none() && tail_lines.is_none() {
        return output.to_string();
    }
    let lines: Vec<&str> = output.lines().collect();
    let total_lines = lines.len();

    let head = head_lines.unwrap_or(0);
    let tail = tail_lines.unwrap_or(0);

    if head + tail >= total_lines {
        return output.to_string();
    }

    let mut result = Vec::new();
    if head > 0 {
        result.extend_from_slice(&lines[..head]);
    }
    if head > 0 || tail > 0 {
        result.push("... [output truncated to save context tokens] ...");
    }
    if tail > 0 {
        result.extend_from_slice(&lines[total_lines - tail..]);
    }

    let mut joined = result.join("\n");
    if output.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
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

    #[tokio::test]
    async fn slices_command_output() {
        let args = serde_json::json!({
            "command": "printf 'line1\nline2\nline3\nline4\nline5\n'",
            "head_lines": 2,
            "tail_lines": 1
        });
        let result = super::execute(".", None, &args).await.expect("command should succeed");
        assert_eq!(result, "line1\nline2\n... [output truncated to save context tokens] ...\nline5\n");
    }
}

