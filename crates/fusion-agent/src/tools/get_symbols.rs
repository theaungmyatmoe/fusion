use std::process::Command;

pub fn execute(cwd: &str, args: &serde_json::Value) -> Result<String, String> {
    let query = args["query"]
        .as_str()
        .ok_or("get_symbols: query is required")?;
    let kind = args["kind"]
        .as_str()
        .unwrap_or("(function|class|interface|type|const|let|export|fn|struct|enum|impl|pub|mod)");

    let pattern = format!(r"({})\s+{}", kind, query);

    let output = Command::new("rg")
        .args([
            "--line-number",
            "--no-heading",
            "--color=never",
            "-e",
            &pattern,
            "--glob",
            "*.rs",
            "--glob",
            "*.ts",
            "--glob",
            "*.tsx",
            "--glob",
            "*.js",
            "--glob",
            "*.jsx",
            "--glob",
            "*.go",
            "--glob",
            "*.py",
            ".",
        ])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Symbol search failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.is_empty() {
        Ok("No symbols found.".to_string())
    } else {
        Ok(stdout.to_string())
    }
}
