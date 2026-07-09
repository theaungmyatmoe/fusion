pub mod read_file;
pub mod write_file;
pub mod search_replace;
pub mod grep;
pub mod get_symbols;
pub mod shell;
pub mod todo;
pub mod search_web;
pub mod fetch_url;

/// Tool registry — maps tool names to their execution logic.
pub struct ToolRegistry {
    cwd: String,
}

impl ToolRegistry {
    pub fn new(cwd: &str) -> Self {
        Self {
            cwd: cwd.to_string(),
        }
    }

    /// Execute a tool by name with the given arguments.
    pub async fn execute(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<String, String> {
        match name {
            "read_file" => read_file::execute(&self.cwd, args),
            "write_file" => write_file::execute(&self.cwd, args),
            "search_replace" => search_replace::execute(&self.cwd, args),
            "grep" => grep::execute(&self.cwd, args),
            "get_symbols" => get_symbols::execute(&self.cwd, args),
            "run_command" => shell::execute(&self.cwd, args).await,
            "todo_write" => todo::execute(args),
            "search_web" => search_web::execute(args).await,
            "fetch_url" => fetch_url::execute(args).await,
            _ => Err(format!("Unknown tool: {}", name)),
        }
    }
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
                "description": "Create a new file or completely overwrite an existing file with new content.",
                "parameters": {
                    "type": "object",
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
                "description": "Precise, safe edit. old_string MUST appear EXACTLY ONCE. This is the only way to edit existing files.",
                "parameters": {
                    "type": "object",
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
                "description": "Execute a shell command. Will prompt for permission unless YOLO mode is on.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Shell command to execute" },
                        "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 30)" }
                    },
                    "required": ["command"]
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
    ]
}
