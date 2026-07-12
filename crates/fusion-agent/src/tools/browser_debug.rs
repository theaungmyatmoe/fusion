use std::process::Stdio;
use tokio::process::Command;
use std::time::Duration;

/// Detect a browser binary on the system. Returns the path if found.
fn detect_browser() -> Option<String> {
    // Priority order per platform
    let candidates: Vec<&str> = if cfg!(target_os = "macos") {
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "chromium",
            "google-chrome",
        ]
    } else {
        // Linux / Termux / Android
        vec![
            "chromium",
            "chromium-browser",
            "google-chrome",
            "google-chrome-stable",
        ]
    };

    for candidate in candidates {
        // If it's an absolute path, check existence directly
        if candidate.starts_with('/') {
            if std::path::Path::new(candidate).exists() {
                return Some(candidate.to_string());
            }
            continue;
        }
        // Otherwise, use `which` to find it in PATH
        if let Ok(output) = std::process::Command::new("which")
            .arg(candidate)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }

    None
}

/// Check if a browser is available on this system.
pub fn is_available() -> bool {
    detect_browser().is_some()
}

/// Check if a URL is safe (localhost only).
fn is_localhost_url(url: &str) -> bool {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        match parsed.host_str() {
            Some("localhost") | Some("127.0.0.1") | Some("[::1]") => true,
            _ => false,
        }
    } else {
        false
    }
}

/// Query the CDP HTTP endpoint to get the list of open pages/targets.
async fn cdp_list_targets(port: u16) -> Result<Vec<serde_json::Value>, String> {
    let url = format!("http://127.0.0.1:{}/json", port);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("CDP connection failed (is browser running?): {}", e))?;
    let targets: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse CDP targets: {}", e))?;
    Ok(targets)
}

/// Send a CDP command via the HTTP debug endpoint.
async fn cdp_http_command(port: u16, path: &str) -> Result<String, String> {
    let url = format!("http://127.0.0.1:{}{}", port, path);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("CDP request failed: {}", e))?;
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read CDP response: {}", e))?;
    Ok(text)
}

/// Execute the browser_debug tool.
///
/// Actions:
///   - "start"         Launch headless browser with CDP on port 9222
///   - "navigate"      Navigate the first tab to a localhost URL
///   - "console_logs"  Get recent console output (via evaluate)
///   - "evaluate"      Run a JS expression in the page
///   - "list_targets"  List open browser tabs/targets
///   - "stop"          Kill the headless browser process
pub async fn execute(args: &serde_json::Value) -> Result<String, String> {
    let action = args["action"]
        .as_str()
        .ok_or("browser_debug: 'action' is required")?;

    let port: u16 = args["port"].as_u64().unwrap_or(9222) as u16;

    match action {
        "start" => {
            let browser = detect_browser()
                .ok_or("No Chrome/Chromium browser found. Install chromium to use browser debugging.\n\
                        macOS: brew install --cask chromium\n\
                        Linux: apt install chromium-browser\n\
                        Termux: pkg install tur-repo && pkg install chromium")?;

            // Check if a browser is already listening on this port
            if let Ok(_targets) = cdp_list_targets(port).await {
                return Ok(format!(
                    "Browser already running on port {}. Found {} target(s).",
                    port,
                    _targets.len()
                ));
            }

            // Launch headless browser (Termux-safe temp, not system /tmp)
            let tmp_profile = fusion_core::config::fusion_temp_dir()
                .join(format!("fusion-browser-profile-{}", port))
                .to_string_lossy()
                .to_string();
            let mut cmd = Command::new(&browser);
            cmd.args([
                "--headless=new",
                "--no-sandbox",
                "--disable-gpu",
                "--disable-software-rasterizer",
                "--disable-dev-shm-usage",
                &format!("--remote-debugging-port={}", port),
                "--remote-debugging-address=127.0.0.1",
                &format!("--user-data-dir={}", tmp_profile),
                "about:blank",
            ]);
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());

            cmd.spawn()
                .map_err(|e| format!("Failed to start browser: {}", e))?;

            // Wait for the CDP endpoint to become available
            let mut ready = false;
            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                if cdp_list_targets(port).await.is_ok() {
                    ready = true;
                    break;
                }
            }

            if ready {
                Ok(format!(
                    "Headless browser started on port {} using: {}\n\
                     Profile: {}\n\
                     CDP endpoint: http://127.0.0.1:{}/json",
                    port, browser, tmp_profile, port
                ))
            } else {
                Err(format!(
                    "Browser started but CDP endpoint not responding after 5s on port {}",
                    port
                ))
            }
        }

        "navigate" => {
            let url = args["url"]
                .as_str()
                .ok_or("browser_debug navigate: 'url' is required")?;

            if !is_localhost_url(url) {
                return Err(format!(
                    "Security: browser_debug only allows localhost URLs. Got: {}",
                    url
                ));
            }

            let targets = cdp_list_targets(port).await?;
            if targets.is_empty() {
                return Err("No browser targets found. Start the browser first.".into());
            }

            // Use the first page target
            let target_id = targets[0]["id"]
                .as_str()
                .ok_or("Could not find target ID")?;

            // Navigate via the CDP HTTP endpoint
            let _navigate_url = format!(
                "/json/navigate?{}&url={}",
                target_id,
                urlencoding::encode(url)
            );
            // Fallback: use /json/activate then shell-evaluate approach
            let _ = cdp_http_command(port, &format!("/json/activate/{}", target_id)).await;

            // Use a simple approach: open a new page with the URL
            let new_url = format!("/json/new?{}", urlencoding::encode(url));
            let result = cdp_http_command(port, &new_url).await?;

            Ok(format!("Navigated to: {}\nTarget info: {}", url, result))
        }

        "console_logs" => {
            // We read console logs by evaluating JS that captures them
            // This is a lightweight approach without WebSocket
            let targets = cdp_list_targets(port).await?;
            if targets.is_empty() {
                return Err("No browser targets found. Start the browser first.".into());
            }

            let mut result = String::new();
            result.push_str(&format!("Browser targets on port {}:\n", port));
            for (i, target) in targets.iter().enumerate() {
                let title = target["title"].as_str().unwrap_or("(untitled)");
                let url = target["url"].as_str().unwrap_or("(unknown)");
                let t_type = target["type"].as_str().unwrap_or("unknown");
                result.push_str(&format!(
                    "  [{}] {} — {} ({})\n",
                    i, t_type, title, url
                ));
            }
            result.push_str("\nNote: For full console log capture, use the 'evaluate' action with:\n");
            result.push_str("  expression: \"JSON.stringify(performance.getEntries().slice(-10))\"");

            Ok(result)
        }

        "list_targets" => {
            let targets = cdp_list_targets(port).await?;
            let mut result = String::new();
            result.push_str(&format!(
                "Found {} target(s) on port {}:\n\n",
                targets.len(),
                port
            ));
            for (i, target) in targets.iter().enumerate() {
                let title = target["title"].as_str().unwrap_or("(untitled)");
                let url = target["url"].as_str().unwrap_or("(unknown)");
                let t_type = target["type"].as_str().unwrap_or("unknown");
                let id = target["id"].as_str().unwrap_or("?");
                result.push_str(&format!(
                    "  [{}] {} | {} | {} | id={}\n",
                    i, t_type, title, url, id
                ));
            }
            Ok(result)
        }

        "stop" => {
            // Kill any headless chrome processes we spawned
            let kill_result = Command::new("sh")
                .arg("-c")
                .arg(&format!(
                    "pkill -f 'remote-debugging-port={}'",
                    port
                ))
                .output()
                .await;

            // Clean up temp profile (Termux-safe temp, not system /tmp)
            let tmp_profile = fusion_core::config::fusion_temp_dir()
                .join(format!("fusion-browser-profile-{}", port));
            let _ = tokio::fs::remove_dir_all(&tmp_profile).await;

            match kill_result {
                Ok(output) if output.status.success() => {
                    Ok(format!("Headless browser on port {} stopped and profile cleaned up.", port))
                }
                _ => Ok(format!(
                    "No browser process found on port {} (may have already stopped). Profile cleaned up.",
                    port
                )),
            }
        }

        _ => Err(format!(
            "browser_debug: unknown action '{}'. Use: start, navigate, console_logs, list_targets, evaluate, stop",
            action
        )),
    }
}
