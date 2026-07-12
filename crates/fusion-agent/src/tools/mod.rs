pub mod apply_patch;
pub mod browser_debug;
pub mod fetch_url;
pub mod get_symbols;
pub mod glob;
pub mod grep;
pub mod read_file;
pub mod search_replace;
pub mod search_web;
pub mod shell;
pub mod todo;
pub mod write_file;
pub mod use_skill;

use std::path::{Path, PathBuf};

/// Resolve the path and check that it stays within the `cwd` workspace directory.
/// To protect against symlink attacks and path traversal, we canonicalize the path's
/// existing ancestors and verify it doesn't escape `cwd`.
pub fn resolve_path_safe(cwd: &str, main_cwd: Option<&str>, path_str: &str) -> Result<PathBuf, String> {
    let p = Path::new(path_str);
    let resolved = if p.is_absolute() {
        if let Some(main) = main_cwd {
            let main_path = Path::new(main);
            if let Ok(relative) = p.strip_prefix(main_path) {
                Path::new(cwd).join(relative)
            } else {
                p.to_path_buf()
            }
        } else {
            p.to_path_buf()
        }
    } else {
        Path::new(cwd).join(path_str)
    };

    let canonical_cwd = std::fs::canonicalize(cwd)
        .map_err(|e| format!("Failed to canonicalize workspace root {}: {}", cwd, e))?;

    let mut current = resolved.as_path();
    let mut canonical_ancestor = None;
    let mut suffix = PathBuf::new();

    loop {
        if current.exists() {
            if let Ok(canonical) = std::fs::canonicalize(current) {
                canonical_ancestor = Some(canonical);
                break;
            }
        }
        if let Some(parent) = current.parent() {
            if let Some(name) = current.file_name() {
                let mut new_suffix = PathBuf::from(name);
                new_suffix.push(suffix);
                suffix = new_suffix;
            }
            current = parent;
        } else {
            break;
        }
    }

    let final_canonical_path = match canonical_ancestor {
        Some(mut ancestor) => {
            ancestor.push(suffix);
            let mut normalized = PathBuf::new();
            for component in ancestor.components() {
                match component {
                    std::path::Component::ParentDir => {
                        normalized.pop();
                    }
                    std::path::Component::CurDir => {}
                    c => normalized.push(c.as_os_str()),
                }
            }
            normalized
        }
        None => {
            let mut normalized = PathBuf::new();
            for component in resolved.components() {
                match component {
                    std::path::Component::ParentDir => {
                        normalized.pop();
                    }
                    std::path::Component::CurDir => {}
                    c => normalized.push(c.as_os_str()),
                }
            }
            normalized
        }
    };

    if !final_canonical_path.starts_with(&canonical_cwd) {
        return Err(format!(
            "Path security violation: resolved path '{:?}' is outside workspace '{:?}'",
            final_canonical_path, canonical_cwd
        ));
    }

    Ok(final_canonical_path)
}


/// Tool registry — maps tool names to their execution logic.
pub struct ToolRegistry {
    cwd: String,
    main_cwd: Option<String>,
    _keenable_api_key: Option<String>,
}

impl ToolRegistry {
    pub fn new(cwd: &str, main_cwd: Option<String>, keenable_api_key: Option<String>) -> Self {
        Self {
            cwd: cwd.to_string(),
            main_cwd,
            _keenable_api_key: keenable_api_key,
        }
    }

    /// Execute a tool by name with the given arguments.
    pub async fn execute(&self, name: &str, args: &serde_json::Value) -> Result<String, String> {
        self.execute_streaming(name, args, None).await
    }

    /// Execute a tool and optionally emit output chunks while it is running.
    pub async fn execute_streaming(
        &self,
        name: &str,
        args: &serde_json::Value,
        output_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Result<String, String> {
        validate_arguments(name, args)?;

        match name {
            "read_file" => read_file::execute(&self.cwd, self.main_cwd.as_deref(), args),
            "write_file" => write_file::execute(&self.cwd, self.main_cwd.as_deref(), args),
            "search_replace" => search_replace::execute(&self.cwd, self.main_cwd.as_deref(), args),
            "apply_patch" => apply_patch::execute(&self.cwd, self.main_cwd.as_deref(), args),
            "grep" => grep::execute(&self.cwd, args),
            "get_symbols" => get_symbols::execute(&self.cwd, args),
            "run_command" => {
                shell::execute_streaming(&self.cwd, self.main_cwd.as_deref(), args, output_tx).await
            }
            "todo_write" => todo::execute(args),
            "search_web" => search_web::execute(args).await,
            "fetch_url" => fetch_url::execute(args).await,
            "glob" => glob::execute(&self.cwd, args),
            "browser_debug" => browser_debug::execute(args).await,
            "use_skill" => use_skill::execute(&self.cwd, args),
            _ => Err(format!("Unknown tool: {}", name)),
        }
    }
}

fn validate_arguments(name: &str, args: &serde_json::Value) -> Result<(), String> {
    let required: &[&str] = match name {
        "write_file" => &["path", "content"],
        "search_replace" => &["path", "old_string", "new_string"],
        "apply_patch" => &["patchText"],
        "read_file" => &["path"],
        "run_command" => &["command"],
        _ => return Ok(()),
    };

    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|field| !args.get(*field).is_some_and(serde_json::Value::is_string))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    let protocol_error = args
        .get("_fusion_tool_error")
        .and_then(serde_json::Value::as_str)
        .map(|reason| format!(" Transport error: {}.", reason))
        .unwrap_or_default();
    Err(format!(
        "Invalid {} call: missing required string field(s): {}.{} Resubmit exactly one corrected call with the documented schema; never retry with {{}}.",
        name,
        missing.join(", "),
        protocol_error,
    ))
}

/// Build the tool schemas to send to the LLM for function calling.
pub fn build_tool_schemas() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read the contents of a file. Use this to understand code before editing.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relative to project root or absolute" }
                    },
                    "required": ["path"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Create a new file or completely overwrite one file. Always provide both path and the complete content. Prefer search_replace for existing files.",
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": { "type": "string", "description": "Path relative to project root" },
                        "content": { "type": "string", "description": "Complete content to write" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "search_replace",
                "description": "Precise, safe edit for existing files. old_string MUST appear EXACTLY ONCE. Prefer this over rewriting a whole file.",
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": { "type": "string" },
                        "old_string": { "type": "string" },
                        "new_string": { "type": "string" }
                    },
                    "required": ["path", "old_string", "new_string"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "apply_patch",
                "description": "Apply a compact multi-file patch. Prefer this for editing existing files. Format: *** Begin Patch, then *** Add File|Update File|Delete File sections, then *** End Patch.",
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "patchText": { "type": "string", "description": "Complete OpenCode-style patch text" }
                    },
                    "required": ["patchText"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "grep",
                "description": "Fast search across the project using ripgrep.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string" },
                        "glob": { "type": "string", "description": "Optional file glob filter" }
                    },
                    "required": ["pattern"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_symbols",
                "description": "LSP-like code intelligence. Finds functions, classes, interfaces etc. using ripgrep.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "kind": { "type": "string", "description": "function|class|interface|type|const|export" }
                    },
                    "required": ["query"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Execute a shell command. PAGER=cat, EDITOR=true, and GIT_EDITOR=true are preset by default to prevent blocking interactive prompts.\n\n\
                GUIDELINES TO PREVENT HANGS:\n\
                - Always insert '--no-pager' immediately after 'git' for read-only commands (e.g. 'git --no-pager log -n 5').\n\
                - Always prepend 'GIT_EDITOR=true ' to git commands that might invoke an editor (e.g. 'GIT_EDITOR=true git rebase').\n\
                - Never run interactive commands or commands that run indefinitely (like dev servers, file watchers). Use non-interactive modes (e.g. -y).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Shell command to execute" },
                        "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 30)" },
                        "head_lines": { "type": "integer", "description": "Optional limit: return only the first N lines of stdout to the LLM (TUI shows full output)" },
                        "tail_lines": { "type": "integer", "description": "Optional limit: return only the last N lines of stdout to the LLM (TUI shows full output)" },
                        "reason": { "type": "string", "description": "Explanation of why this command is being run (displayed to user if authorization is needed)" }
                    },
                    "required": ["command"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "use_skill",
                "description": "Load the complete guidelines and best practices for a specialized skill by name.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "The exact name of the skill to load (e.g. remotion-best-practices)" }
                    },
                    "required": ["name"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "todo_write",
                "description": "Create or update the visible todo list to track your work.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "todos": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "content": { "type": "string" },
                                    "status": { "type": "string", "enum": ["pending", "in_progress", "done"] }
                                }
                            }
                        }
                    },
                    "required": ["todos"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "search_web",
                "description": "Search the web for up-to-date documentation, API syntax, packages, or error solutions.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "The search query string" }
                    },
                    "required": ["query"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "fetch_url",
                "description": "Download and read the clean text contents of a URL (webpage, API docs, JSON).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "The absolute URL to fetch" }
                    },
                    "required": ["url"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "glob",
                "description": "Find files by glob pattern (e.g. src/**/*.rs or **/*.json) within the project.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Glob wildcard pattern to match (e.g. *.rs)" },
                        "limit": { "type": "integer", "description": "Maximum file paths to return (default 100)" }
                    },
                    "required": ["pattern"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "browser_debug",
                "description": "Optional browser DevTools debugging. Launch a headless Chrome/Chromium, navigate to localhost pages, inspect targets, and read console output via CDP. Only works with localhost URLs for safety. Use action 'start' first, then 'navigate', 'list_targets', 'console_logs', or 'stop'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["start", "navigate", "list_targets", "console_logs", "stop"],
                            "description": "The browser debug action to perform"
                        },
                        "url": {
                            "type": "string",
                            "description": "URL to navigate to (localhost only). Required for 'navigate' action."
                        },
                        "port": {
                            "type": "integer",
                            "description": "CDP debugging port (default 9222)"
                        }
                    },
                    "required": ["action"]
                }
            }
        }),
    ]
}

/// Build tool schemas filtered to only the tools a given persona is allowed to use.
/// If `allowed_tools` is empty, returns the full set.
pub fn build_tool_schemas_for_persona(allowed_tools: &[&str]) -> Vec<serde_json::Value> {
    if allowed_tools.is_empty() {
        return build_tool_schemas();
    }
    build_tool_schemas()
        .into_iter()
        .filter(|schema| {
            schema
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .is_some_and(|name| allowed_tools.contains(&name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::validate_arguments;

    #[test]
    fn rejects_empty_write_file_arguments_with_schema_feedback() {
        let args = serde_json::json!({
            "_fusion_tool_error": "empty arguments"
        });
        let error = validate_arguments("write_file", &args).unwrap_err();

        assert!(error.contains("path, content"));
        assert!(error.contains("Transport error: empty arguments"));
        assert!(error.contains("never retry with {}"));
    }
}
