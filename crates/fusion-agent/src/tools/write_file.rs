use serde::Deserialize;
use std::fs;
use super::resolve_path_safe;

#[derive(Deserialize)]
struct WriteFileArgs {
    path: String,
    content: String,
}

pub fn execute(cwd: &str, main_cwd: Option<&str>, args: &serde_json::Value) -> Result<String, String> {
    let args: WriteFileArgs = serde_json::from_value(args.clone())
        .map_err(|e| format!("Invalid write_file arguments: {}", e))?;

    let full_path = resolve_path_safe(cwd, main_cwd, &args.path)?;

    // Auto-create parent directories
    if let Some(parent) = full_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directories: {}", e))?;
        }
    }

    fs::write(&full_path, &args.content)
        .map_err(|e| format!("Failed to write {}: {}", args.path, e))?;

    Ok(format!("SUCCESS. Wrote file to {}.", args.path))
}

