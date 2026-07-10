use fusion_core::config::Config;
use fusion_core::error::FusionError;
use fusion_llm::client::{ChatMessage, ChatOptions, create_llm_client, LlmClient};

use crate::tools::{ToolRegistry, build_tool_schemas};

/// Events emitted by the agent for the TUI to render.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    Thinking(String),
    TextDelta(String),
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
    pub async fn process(
        &mut self,
        user_message: &str,
        tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<(), FusionError> {
        // Initialize system prompt on first message
        if self.messages.is_empty() {
            let mut sys_prompt = String::from(
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
                 6. Use todo tools to track your progress visibly.\n\n\
                 IMPORTANT: When you have gathered enough information and made the needed changes, \n\
                 you MUST provide a final text response summarizing your work. \n\
                 Do not keep calling tools indefinitely — be efficient and wrap up."
            );

            // Load any specialized local or global skills
            let skills = fusion_core::config::load_skills(&self.cwd);
            if !skills.is_empty() {
                sys_prompt.push_str("\n\nAVAILABLE SPECIALIZED SKILLS AND BEST PRACTICES:\n");
                for (name, content) in skills {
                    sys_prompt.push_str(&format!("--- SKILL: {} ---\n{}\n\n", name, content));
                }
            }

            self.messages.push(ChatMessage::system(sys_prompt));
        }

        self.messages.push(ChatMessage::user(user_message));

        let tool_schemas = build_tool_schemas();
        let max_rounds = 25;

        for round in 0..max_rounds {
            // Pacing delay to avoid triggering Cloudflare burst rate limits during tool use
            if round > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            }

            // Inject a wrap-up nudge when running low on rounds
            if round == max_rounds - 2 {
                self.messages.push(ChatMessage::system(
                    "IMPORTANT: You are running low on remaining rounds. \
                     Finish up your current work and provide a final text answer \
                     summarizing what you have done. Do NOT call more tools unless absolutely necessary."
                ));
            }

            // Boost max_tokens on final rounds so the model has enough budget to conclude
            let max_tokens = if round >= max_rounds - 3 {
                Some(16384)
            } else {
                None
            };

            let options = ChatOptions {
                messages: self.messages.clone(),
                tools: Some(tool_schemas.clone()),
                temperature: Some(0.4),
                max_tokens,
            };

            let (llm_tx, mut llm_rx) = tokio::sync::mpsc::unbounded_channel();
            let tx_clone = tx.clone();
            
            // Spawn background task to forward streaming events to TUI in real-time
            let forwarder = tokio::spawn(async move {
                while let Some(llm_event) = llm_rx.recv().await {
                    match llm_event {
                        fusion_llm::client::LlmEvent::Thinking(chunk) => {
                            let _ = tx_clone.send(AgentEvent::Thinking(chunk));
                        }
                        fusion_llm::client::LlmEvent::TextDelta(chunk) => {
                            let _ = tx_clone.send(AgentEvent::TextDelta(chunk));
                        }
                    }
                }
            });

            let result = match self.llm.chat(options, Some(llm_tx)).await {
                Ok(res) => res,
                Err(e) => {
                    let _ = forwarder.await;
                    return Err(e);
                }
            };
            let _ = forwarder.await;

            // Handle tool calls
            if !result.tool_calls.is_empty() {
                // Add assistant message with tool calls populated
                self.messages
                    .push(ChatMessage::assistant_with_tools(result.content.clone(), result.tool_calls.clone()));

                for tc in &result.tool_calls {
                    let args_str = serde_json::to_string(&tc.arguments).unwrap_or_default();
                    let preview = if args_str.chars().count() > 200 {
                        let truncated: String = args_str.chars().take(200).collect();
                        format!("{}...", truncated)
                    } else {
                        args_str.clone()
                    };

                    let _ = tx.send(AgentEvent::ToolCall {
                        name: tc.name.clone(),
                        args_preview: preview,
                    });

                    // Execute the tool
                    let output = self
                        .tool_registry
                        .execute(&tc.name, &tc.arguments)
                        .await
                        .unwrap_or_else(|e| format!("Tool execution error: {}", e));

                    let _ = tx.send(AgentEvent::ToolResult {
                        name: tc.name.clone(),
                        output: output.clone(),
                    });

                    // Handle todo updates
                    if tc.name == "todo_write" {
                        if let Ok(items) =
                            serde_json::from_value::<Vec<TodoItem>>(tc.arguments["todos"].clone())
                        {
                            self.todos = items.clone();
                            let _ = tx.send(AgentEvent::TodoUpdate(items));
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
            let _ = tx.send(AgentEvent::FinalResponse(final_text));
            return Ok(());
        }

        // Force one final LLM call WITHOUT tools to guarantee a text summary
        self.messages.push(ChatMessage::system(
            "You have exhausted all available tool rounds. \
             You MUST now provide a final text response summarizing \
             what you accomplished and any remaining work. \
             Do NOT attempt any tool calls."
        ));

        let final_options = ChatOptions {
            messages: self.messages.clone(),
            tools: None, // No tools — forces text-only response
            temperature: Some(0.4),
            max_tokens: Some(16384),
        };

        let (llm_tx, mut llm_rx) = tokio::sync::mpsc::unbounded_channel();
        let tx_clone = tx.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(llm_event) = llm_rx.recv().await {
                match llm_event {
                    fusion_llm::client::LlmEvent::Thinking(chunk) => {
                        let _ = tx_clone.send(AgentEvent::Thinking(chunk));
                    }
                    fusion_llm::client::LlmEvent::TextDelta(chunk) => {
                        let _ = tx_clone.send(AgentEvent::TextDelta(chunk));
                    }
                }
            }
        });

        match self.llm.chat(final_options, Some(llm_tx)).await {
            Ok(result) => {
                let _ = forwarder.await;
                let final_text = if result.content.is_empty() {
                    "Agent completed all rounds. No further summary available.".to_string()
                } else {
                    result.content
                };
                self.messages.push(ChatMessage::assistant(&final_text));
                let _ = tx.send(AgentEvent::FinalResponse(final_text));
            }
            Err(_) => {
                let _ = forwarder.await;
                let msg = "Agent completed all available rounds.".to_string();
                let _ = tx.send(AgentEvent::FinalResponse(msg));
            }
        }
        Ok(())
    }

    pub fn get_todos(&self) -> &[TodoItem] {
        &self.todos
    }

    pub fn get_history(&self) -> &[ChatMessage] {
        &self.messages
    }
}
