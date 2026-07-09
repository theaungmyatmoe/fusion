use std::process::Command;

pub fn execute(cwd: &str, args: &serde_json::Value) -> Result<String, String> {
    let pattern = args["pattern"]
        .as_str()
        .ok_or("grep: pattern is required")?;
    let glob = args["glob"].as_str();

    let mut cmd = Command::new("rg");
    cmd.arg("--line-number")
        .arg("--no-heading")
        .arg("--color=never");

    if let Some(g) = glob {
        cmd.arg("--glob").arg(g);
    }

    cmd.arg(pattern).arg(".").current_dir(cwd);

    let output = cmd.output().map_err(|e| {
        format!(
            "grep failed (is ripgrep installed? `pkg install ripgrep`): {}",
            e
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.is_empty() {
        Ok("No matches.".to_string())
    } else {
        Ok(stdout.to_string())
    }
}
