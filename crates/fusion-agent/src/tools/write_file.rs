use std::fs;
use std::path::Path;

pub fn execute(cwd: &str, main_cwd: Option<&str>, args: &serde_json::Value) -> Result<String, String> {
    let path_str = args["path"]
        .as_str()
        .ok_or("write_file: path is required")?;
    let content = args["content"]
        .as_str()
        .ok_or("write_file: content is required")?;

    let full_path = resolve_path(cwd, main_cwd, path_str);

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

fn resolve_path(cwd: &str, main_cwd: Option<&str>, path: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        if let Some(main) = main_cwd {
            let main_path = Path::new(main);
            if let Ok(relative) = p.strip_prefix(main_path) {
                return Path::new(cwd).join(relative);
            }
        }
        p.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    }
}
