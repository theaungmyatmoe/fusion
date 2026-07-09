use std::fs;
use std::path::Path;

use fusion_core::search_replace::apply_search_replace;

pub fn execute(cwd: &str, args: &serde_json::Value) -> Result<String, String> {
    let path_str = args["path"]
        .as_str()
        .ok_or("search_replace: path is required")?;
    let old_string = args["old_string"]
        .as_str()
        .ok_or("search_replace: old_string is required")?;
    let new_string = args["new_string"]
        .as_str()
        .ok_or("search_replace: new_string is required")?;

    let full_path = resolve_path(cwd, path_str);

    if !full_path.exists() {
        return Err(format!("File not found: {}", path_str));
    }

    let content = fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read {}: {}", path_str, e))?;

    let (new_content, result) = apply_search_replace(&content, old_string, new_string)?;

    fs::write(&full_path, new_content)
        .map_err(|e| format!("Failed to write {}: {}", path_str, e))?;

    Ok(format!(
        "SUCCESS. Applied edit to {}.\n{}",
        path_str, result.diff
    ))
}

fn resolve_path(cwd: &str, path: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    }
}
