use std::fs;
use std::path::Path;

pub fn execute(cwd: &str, main_cwd: Option<&str>, args: &serde_json::Value) -> Result<String, String> {
    let path_str = args["path"]
        .as_str()
        .ok_or("read_file: path is required")?;

    let full_path = resolve_path(cwd, main_cwd, path_str);

    if !full_path.exists() {
        return Err(format!("File not found: {}", path_str));
    }

    fs::read_to_string(&full_path).map_err(|e| format!("Failed to read {}: {}", path_str, e))
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
