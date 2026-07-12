use serde::Deserialize;
use std::fs;
use super::resolve_path_safe;
use fusion_core::search_replace::apply_search_replace;

#[derive(Deserialize)]
struct SearchReplaceArgs {
    path: String,
    old_string: String,
    new_string: String,
}

pub fn execute(cwd: &str, main_cwd: Option<&str>, args: &serde_json::Value) -> Result<String, String> {
    let args: SearchReplaceArgs = serde_json::from_value(args.clone())
        .map_err(|e| format!("Invalid search_replace arguments: {}", e))?;

    let full_path = resolve_path_safe(cwd, main_cwd, &args.path)?;

    if !full_path.exists() {
        return Err(format!("File not found: {}", args.path));
    }

    let content = fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read {}: {}", args.path, e))?;

    let (new_content, result) = apply_search_replace(&content, &args.old_string, &args.new_string)?;

    fs::write(&full_path, new_content)
        .map_err(|e| format!("Failed to write {}: {}", args.path, e))?;

    Ok(format!(
        "SUCCESS. Applied edit to {}.\n{}",
        args.path, result.diff
    ))
}

