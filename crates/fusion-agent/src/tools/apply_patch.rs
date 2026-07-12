use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
enum Operation {
    Add {
        path: String,
        content: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        chunks: Vec<Chunk>,
    },
    Delete {
        path: String,
    },
}

#[derive(Debug, Default)]
struct Chunk {
    lines: Vec<PatchLine>,
}

#[derive(Debug)]
enum PatchLine {
    Context(String),
    Add(String),
    Remove(String),
}

struct Change {
    source: PathBuf,
    destination: Option<PathBuf>,
    content: Option<String>,
    status: char,
    display_path: String,
}

pub fn execute(
    cwd: &str,
    main_cwd: Option<&str>,
    args: &serde_json::Value,
) -> Result<String, String> {
    let patch = args["patchText"]
        .as_str()
        .ok_or("apply_patch: patchText is required")?;
    let operations = parse_patch(patch)?;
    let mut changes = Vec::with_capacity(operations.len());

    for operation in operations {
        match operation {
            Operation::Add { path, content } => {
                let target = resolve_workspace_path(cwd, main_cwd, &path)?;
                if target.exists() {
                    return Err(format!("apply_patch: file already exists: {}", path));
                }
                changes.push(Change {
                    source: target,
                    destination: None,
                    content: Some(ensure_trailing_newline(content)),
                    status: 'A',
                    display_path: path,
                });
            }
            Operation::Delete { path } => {
                let target = resolve_workspace_path(cwd, main_cwd, &path)?;
                if !target.is_file() {
                    return Err(format!("apply_patch: file not found for delete: {}", path));
                }
                changes.push(Change {
                    source: target,
                    destination: None,
                    content: None,
                    status: 'D',
                    display_path: path,
                });
            }
            Operation::Update {
                path,
                move_to,
                chunks,
            } => {
                let source = resolve_workspace_path(cwd, main_cwd, &path)?;
                let current = fs::read_to_string(&source)
                    .map_err(|e| format!("apply_patch: failed to read {}: {}", path, e))?;
                let content = apply_chunks(&path, &current, &chunks)?;
                let destination = move_to
                    .as_deref()
                    .map(|target| resolve_workspace_path(cwd, main_cwd, target))
                    .transpose()?;
                let display_path = move_to.unwrap_or(path);
                changes.push(Change {
                    source,
                    destination,
                    content: Some(content),
                    status: 'M',
                    display_path,
                });
            }
        }
    }

    for change in &changes {
        match (&change.content, &change.destination) {
            (None, _) => fs::remove_file(&change.source).map_err(|e| {
                format!(
                    "apply_patch: failed to delete {}: {}",
                    change.display_path, e
                )
            })?,
            (Some(content), Some(destination)) => {
                write_with_parents(destination, content)?;
                if destination != &change.source {
                    fs::remove_file(&change.source).map_err(|e| {
                        format!(
                            "apply_patch: failed to remove moved source {}: {}",
                            change.source.display(),
                            e
                        )
                    })?;
                }
            }
            (Some(content), None) => write_with_parents(&change.source, content)?,
        }
    }

    let summary = changes
        .iter()
        .map(|change| format!("{} {}", change.status, change.display_path))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "Success. Updated the following files:\n{}",
        summary
    ))
}

fn parse_patch(patch: &str) -> Result<Vec<Operation>, String> {
    let normalized = patch.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    if lines.first().copied() != Some("*** Begin Patch") {
        return Err("apply_patch: patch must start with '*** Begin Patch'".to_string());
    }
    if lines.last().copied() != Some("*** End Patch") {
        return Err("apply_patch: patch must end with '*** End Patch'".to_string());
    }

    let mut operations = Vec::new();
    let mut index = 1;
    while index + 1 < lines.len() {
        let header = lines[index];
        if let Some(path) = header.strip_prefix("*** Add File: ") {
            index += 1;
            let mut content = Vec::new();
            while index < lines.len() && !lines[index].starts_with("*** ") {
                let line = lines[index].strip_prefix('+').ok_or_else(|| {
                    format!(
                        "apply_patch: added file lines must start with '+': {}",
                        lines[index]
                    )
                })?;
                content.push(line.to_string());
                index += 1;
            }
            operations.push(Operation::Add {
                path: path.trim().to_string(),
                content: content.join("\n"),
            });
            continue;
        }
        if let Some(path) = header.strip_prefix("*** Delete File: ") {
            operations.push(Operation::Delete {
                path: path.trim().to_string(),
            });
            index += 1;
            continue;
        }
        if let Some(path) = header.strip_prefix("*** Update File: ") {
            index += 1;
            let mut move_to = None;
            if index < lines.len() {
                if let Some(target) = lines[index].strip_prefix("*** Move to: ") {
                    move_to = Some(target.trim().to_string());
                    index += 1;
                }
            }
            let mut chunks = Vec::new();
            let mut current = Chunk::default();
            while index < lines.len() && !lines[index].starts_with("*** ") {
                let line = lines[index];
                if line.starts_with("@@") {
                    if !current.lines.is_empty() {
                        chunks.push(current);
                        current = Chunk::default();
                    }
                } else if let Some(value) = line.strip_prefix('+') {
                    current.lines.push(PatchLine::Add(value.to_string()));
                } else if let Some(value) = line.strip_prefix('-') {
                    current.lines.push(PatchLine::Remove(value.to_string()));
                } else if let Some(value) = line.strip_prefix(' ') {
                    current.lines.push(PatchLine::Context(value.to_string()));
                } else if line.is_empty() {
                    current.lines.push(PatchLine::Context(String::new()));
                } else {
                    return Err(format!("apply_patch: invalid update line: {}", line));
                }
                index += 1;
            }
            if !current.lines.is_empty() {
                chunks.push(current);
            }
            if chunks.is_empty() {
                return Err(format!(
                    "apply_patch: update has no chunks: {}",
                    path.trim()
                ));
            }
            operations.push(Operation::Update {
                path: path.trim().to_string(),
                move_to,
                chunks,
            });
            continue;
        }
        return Err(format!("apply_patch: invalid operation header: {}", header));
    }

    if operations.is_empty() {
        return Err("apply_patch: empty patch".to_string());
    }
    Ok(operations)
}

fn apply_chunks(path: &str, current: &str, chunks: &[Chunk]) -> Result<String, String> {
    let had_trailing_newline = current.ends_with('\n');
    let mut lines: Vec<String> = current
        .strip_suffix('\n')
        .unwrap_or(current)
        .split('\n')
        .map(str::to_string)
        .collect();
    let mut cursor = 0;

    for chunk in chunks {
        let old: Vec<String> = chunk
            .lines
            .iter()
            .filter_map(|line| match line {
                PatchLine::Context(value) | PatchLine::Remove(value) => Some(value.clone()),
                PatchLine::Add(_) => None,
            })
            .collect();
        let new: Vec<String> = chunk
            .lines
            .iter()
            .filter_map(|line| match line {
                PatchLine::Context(value) | PatchLine::Add(value) => Some(value.clone()),
                PatchLine::Remove(_) => None,
            })
            .collect();
        if old.is_empty() {
            return Err(format!(
                "apply_patch: update chunk has no context in {}",
                path
            ));
        }

        let position = find_sequence(&lines, &old, cursor)
            .or_else(|| find_sequence(&lines, &old, 0))
            .ok_or_else(|| format!("apply_patch: chunk context not found in {}", path))?;
        lines.splice(position..position + old.len(), new.iter().cloned());
        cursor = position + new.len();
    }

    let mut output = lines.join("\n");
    if had_trailing_newline {
        output.push('\n');
    }
    Ok(output)
}

fn find_sequence(lines: &[String], needle: &[String], start: usize) -> Option<usize> {
    if needle.len() > lines.len() {
        return None;
    }
    (start..=lines.len() - needle.len())
        .find(|&index| lines[index..index + needle.len()] == *needle)
}

fn ensure_trailing_newline(mut content: String) -> String {
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content
}

fn write_with_parents(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("apply_patch: failed to create {}: {}", parent.display(), e))?;
    }
    fs::write(path, content)
        .map_err(|e| format!("apply_patch: failed to write {}: {}", path.display(), e))
}

fn resolve_workspace_path(
    cwd: &str,
    main_cwd: Option<&str>,
    path: &str,
) -> Result<PathBuf, String> {
    // Share the same jail + Termux allowlist as read/write/search_replace.
    super::resolve_path_safe(cwd, main_cwd, path)
}

#[cfg(test)]
mod tests {
    use super::execute;

    #[test]
    fn adds_updates_and_deletes_files() {
        let root = std::env::temp_dir().join(format!("fusion-apply-patch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("old.txt"), "before\nkeep\n").unwrap();
        std::fs::write(root.join("delete.txt"), "remove me\n").unwrap();
        let patch = "*** Begin Patch\n*** Add File: new.txt\n+hello\n+world\n*** Update File: old.txt\n@@\n-before\n+after\n keep\n*** Delete File: delete.txt\n*** End Patch";

        let result = execute(
            root.to_str().unwrap(),
            None,
            &serde_json::json!({ "patchText": patch }),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(root.join("new.txt")).unwrap(),
            "hello\nworld\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("old.txt")).unwrap(),
            "after\nkeep\n"
        );
        assert!(!root.join("delete.txt").exists());
        assert!(result.contains("A new.txt"));
        assert!(result.contains("M old.txt"));
        assert!(result.contains("D delete.txt"));
        let _ = std::fs::remove_dir_all(root);
    }
}
