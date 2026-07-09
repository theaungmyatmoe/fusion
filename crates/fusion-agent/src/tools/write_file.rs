use std::fs;
use std::path::Path;

pub fn execute(cwd: &str, args: &serde_json::Value) -> Result<String, String> {
    let path_str = args["path"]
        .as_str()
        .ok_or("write_file: path is required")?;
    let content = args["content"]
        .as_str()
        .ok_or("write_file: content is required")?;

    let full_path = resolve_path(cwd, path_str);

    // Auto-create parent directories
    if let Some(parent) = full_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directories: {}", e))?;
        }
    }

    fs::write(&full_path, content)
        .map_err(|e| format!("Failed to write {}: {}", path_str, e))?;

    Ok(format!("SUCCESS. Wrote file to {}.", path_str))
}

fn resolve_path(cwd: &str, path: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    }
}
