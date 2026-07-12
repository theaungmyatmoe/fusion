use serde::Deserialize;
use std::fs;
use super::resolve_path_safe;

#[derive(Deserialize)]
struct ReadFileArgs {
    path: String,
}

pub fn execute(cwd: &str, main_cwd: Option<&str>, args: &serde_json::Value) -> Result<String, String> {
    let args: ReadFileArgs = serde_json::from_value(args.clone())
        .map_err(|e| format!("Invalid read_file arguments: {}", e))?;

    let full_path = resolve_path_safe(cwd, main_cwd, &args.path)?;

    if !full_path.exists() {
        return Err(format!("File not found: {}", args.path));
    }

    fs::read_to_string(&full_path).map_err(|e| format!("Failed to read {}: {}", args.path, e))
}

