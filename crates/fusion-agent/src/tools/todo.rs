use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TodoItem {
    content: String,
    status: String,
}

pub fn execute(args: &serde_json::Value) -> Result<String, String> {
    let todos: Vec<TodoItem> = serde_json::from_value(args["todos"].clone())
        .map_err(|e| format!("Failed to parse todos: {}", e))?;

    let display: String = todos
        .iter()
        .map(|t| {
            let icon = match t.status.as_str() {
                "done" => "✓",
                "in_progress" => "→",
                _ => "○",
            };
            format!("  {} {}", icon, t.content)
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!("Todos updated. Current list:\n{}", display))
}
