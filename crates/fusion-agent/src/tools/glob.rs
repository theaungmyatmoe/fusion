use std::path::Path;
use serde_json::Value;

/// Find files by glob pattern within the workspace directory.
pub fn execute(cwd: &str, args: &Value) -> Result<String, String> {
    let pattern = args["pattern"]
        .as_str()
        .ok_or("glob: pattern is required")?;
    let limit = args["limit"].as_u64().unwrap_or(100) as usize;

    let mut matched_files = Vec::new();
    let root = Path::new(cwd);

    visit_dirs(root, &mut |path| {
        let rel_path = path.strip_prefix(root).unwrap_or(path).to_string_lossy().to_string();
        if match_pattern(&rel_path, pattern) {
            matched_files.push(rel_path);
        }
    }).map_err(|e| format!("Failed to traverse directory: {}", e))?;

    matched_files.sort();
    if matched_files.len() > limit {
        matched_files.truncate(limit);
    }

    if matched_files.is_empty() {
        Ok("No files found matching the pattern.".to_string())
    } else {
        Ok(matched_files.join("\n"))
    }
}

// Helper to recursively walk directories, ignoring binary / dependency folders
fn visit_dirs(dir: &Path, cb: &mut dyn FnMut(&Path)) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name == "target" || name == "node_modules" || name == ".git" || name == "reference" {
                    continue;
                }
                visit_dirs(&path, cb)?;
            } else {
                cb(&path);
            }
        }
    }
    Ok(())
}

// Simple glob matcher supporting '*' wildcards
fn match_pattern(path: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return path == pattern;
    }

    let mut current_idx = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(pos) = path[current_idx..].find(part) {
            if i == 0 && pos != 0 {
                return false;
            }
            current_idx += pos + part.len();
        } else {
            return false;
        }
    }

    if let Some(last) = parts.last() {
        if !last.is_empty() && !path.ends_with(last) {
            return false;
        }
    }

    true
}
