use fusion_core::config::Config;
use fusion_core::error::FusionError;
use fusion_llm::client::{ChatMessage, ChatOptions, ChatResult, create_llm_client, LlmClient};

use crate::tools::{ToolRegistry, build_tool_schemas};

/// Events emitted by the agent for the TUI to render.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    Thinking(String),
    ToolCall { name: String, args_preview: String },
    ToolResult { name: String, output: String },
    FinalResponse(String),
    TodoUpdate(Vec<TodoItem>),
}

/// A todo item tracked by the agent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub content: String,
    pub status: String, // "pending", "in_progress", "done"
}

/// The core agent that manages conversation, tool calls, and LLM interactions.
pub struct Agent {
    llm: LlmClient,
    messages: Vec<ChatMessage>,
    todos: Vec<TodoItem>,
    #[allow(dead_code)]
    cwd: String,
    tool_registry: ToolRegistry,
}

impl Agent {
    pub fn new(config: &Config, cwd: String) -> Self {
        let llm = create_llm_client(config);
        let tool_registry = ToolRegistry::new(&cwd);

        Self {
            llm,
            messages: Vec::new(),
            todos: Vec::new(),
            cwd,
            tool_registry,
        }
    }

    /// Process a user message through the agent loop.
    /// Returns a list of events and the final response.
    pub async fn process(&mut self, user_message: &str) -> Result<Vec<AgentEvent>, FusionError> {
        let mut events = Vec::new();

        // Initialize system prompt on first message
        if self.messages.is_empty() {
            self.messages.push(ChatMessage::system(
                "You are Fusion, a powerful, autonomous coding agent optimized for terminal environments.\n\n\
                 ENVIRONMENT:\n\
                 You are running inside a terminal (CLI). Your text output is rendered in a plain terminal — not a browser, not a rich text editor.\n\
                 - Use plain text only. No markdown tables, no HTML, no images, no colored text.\n\
                 - Use simple markers like dashes (-) or asterisks (*) for lists.\n\
                 - Use indentation and blank lines for structure.\n\
                 - Keep lines under 100 characters when possible.\n\
                 - Use backticks for inline code and triple backticks for code blocks — these are rendered.\n\
                 - Never use unicode box-drawing, fancy borders, or ASCII art in your responses.\n\n\
                 WORKFLOW:\n\
                 1. Understand the request.\n\
                 2. Use tools (read_file, grep, search_replace, shell, get_symbols) to explore the codebase.\n\
                 3. Use search_replace for edits — it requires old_string to match exactly once for safety.\n\
                 4. Keep edits minimal, safe, and focused.\n\
                 5. Run tests or compilation steps via shell to verify correctness.\n\
                 6. Use todo tools to track your progress visibly."
            ));
        }

        self.messages.push(ChatMessage::user(user_message));

        let tool_schemas = build_tool_schemas();
        let max_rounds = 10;

        for round in 0..max_rounds {
            // Pacing delay to avoid triggering Cloudflare burst rate limits during tool use
            if round > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            }

            let options = ChatOptions {
                messages: self.messages.clone(),
                tools: Some(tool_schemas.clone()),
                temperature: Some(0.4),
                max_tokens: None,
            };

            let result: ChatResult = self.llm.chat(options).await?;

            // Emit thinking if present
            if let Some(ref reasoning) = result.reasoning_content {
                events.push(AgentEvent::Thinking(reasoning.clone()));
            }

            // Handle tool calls
            if !result.tool_calls.is_empty() {
                // Add assistant message with tool calls populated
                self.messages
                    .push(ChatMessage::assistant_with_tools(result.content.clone(), result.tool_calls.clone()));

                for tc in &result.tool_calls {
                    let args_str = serde_json::to_string(&tc.arguments).unwrap_or_default();
                    let preview = if args_str.len() > 200 {
                        format!("{}...", &args_str[..200])
                    } else {
                        args_str.clone()
                    };

                    events.push(AgentEvent::ToolCall {
                        name: tc.name.clone(),
                        args_preview: preview,
                    });

                    // Execute the tool
                    let output = self
                        .tool_registry
                        .execute(&tc.name, &tc.arguments)
                        .await
                        .unwrap_or_else(|e| format!("Tool execution error: {}", e));

                    events.push(AgentEvent::ToolResult {
                        name: tc.name.clone(),
                        output: output.clone(),
                    });

                    // Handle todo updates
                    if tc.name == "todo_write" {
                        if let Ok(items) =
                            serde_json::from_value::<Vec<TodoItem>>(tc.arguments["todos"].clone())
                        {
                            self.todos = items.clone();
                            events.push(AgentEvent::TodoUpdate(items));
                        }
                    }

                    self.messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: format!("Tool {} result:\n{}", tc.name, output),
                        name: Some(tc.name.clone()),
                        tool_call_id: Some(tc.id.clone()),
                        tool_calls: None,
                    });
                }
                continue; // next round
            }

            // Final response (no tool calls)
            let final_text = if result.content.is_empty() {
                "(no response)".to_string()
            } else {
                result.content.clone()
            };

            self.messages.push(ChatMessage::assistant(&final_text));
            events.push(AgentEvent::FinalResponse(final_text));
            return Ok(events);
        }

        let msg = "Agent reached max rounds without final answer.".to_string();
        events.push(AgentEvent::FinalResponse(msg));
        Ok(events)
    }

    pub fn get_todos(&self) -> &[TodoItem] {
        &self.todos
    }

    pub fn get_history(&self) -> &[ChatMessage] {
        &self.messages
    }
}
