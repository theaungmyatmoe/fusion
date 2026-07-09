use serde::{Deserialize, Serialize};
use futures::StreamExt;
use reqwest_eventsource::{Event, EventSource};

use fusion_core::config::{Config, Provider, is_cloudflare_model};
use fusion_core::error::FusionError;

/// A single chat message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system", "user", "assistant", "tool"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: Some(tool_calls),
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

/// Options for a chat completion request.
#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub messages: Vec<ChatMessage>,
    pub tools: Option<Vec<serde_json::Value>>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

/// A parsed tool call from the model response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Result of a chat completion.
#[derive(Debug, Clone)]
pub struct ChatResult {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

/// Streaming token/reasoning event emitted to the agent.
#[derive(Debug, Clone)]
pub enum LlmEvent {
    Thinking(String),
    TextDelta(String),
}

/// Unified LLM client that routes to the correct provider.
pub struct LlmClient {
    config: Config,
}

impl LlmClient {
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Retrieve the currently active model ID.
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Send a chat completion request. If event_tx is provided, it streams chunks in real-time.
    pub async fn chat(
        &self,
        options: ChatOptions,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<LlmEvent>>,
    ) -> Result<ChatResult, FusionError> {
        let account_id = self.config.cloudflare_account_id.clone().unwrap_or_default();
        let api_key = self.config.api_key.clone();

        let is_cf = self.config.provider == Provider::Cloudflare || is_cloudflare_model(&self.config.model);
        
        let (url, auth_header) = if is_cf {
            if account_id.is_empty() {
                return Err(FusionError::Llm(
                    "CLOUDFLARE_ACCOUNT_ID is required when using Cloudflare Workers AI.\n\
                     Tip: export CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN, or set them in fusion.toml \
                     under [provider.cloudflare]."
                        .into(),
                ));
            }
            if api_key.is_empty() {
                return Err(FusionError::Llm(
                    "CLOUDFLARE_API_TOKEN is required for Cloudflare Workers AI.\n\
                     Tip: export CLOUDFLARE_API_TOKEN, or set api_key in fusion.toml \
                     under [provider.cloudflare]."
                        .into(),
                ));
            }
            let url = format!(
                "https://api.cloudflare.com/client/v4/accounts/{}/ai/v1/chat/completions",
                account_id
            );
            (url, format!("Bearer {}", api_key))
        } else {
            let base_url = if self.config.base_url.is_empty() {
                "https://api.openai.com/v1".to_string()
            } else {
                self.config.base_url.clone()
            };
            let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
            (url, format!("Bearer {}", api_key))
        };

        // Determine max_tokens
        let max_tokens = options.max_tokens.or_else(|| {
            fusion_core::models::lookup_model(&self.config.model)
                .and_then(|info| info.max_tokens_for(fusion_core::models::TokenLevel::Normal))
                .or(Some(4096))
        });

        // Construct request payload
        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": options.messages,
            "temperature": options.temperature.unwrap_or(0.4),
            "stream": true,
        });

        if let Some(max) = max_tokens {
            body["max_tokens"] = serde_json::json!(max);
        }

        if let Some(ref tools) = options.tools {
            body["tools"] = serde_json::json!(tools);
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let request_builder = client
            .post(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .json(&body);

        let mut event_source = EventSource::new(request_builder)
            .map_err(|e| FusionError::Llm(format!("Failed to connect to SSE stream: {}", e)))?;

        let mut accumulated_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<ToolCallBuilder> = Vec::new();

        while let Some(event) = event_source.next().await {
            match event {
                Ok(Event::Open) => {}
                Ok(Event::Message(msg)) => {
                    let data = msg.data.trim();
                    if data == "[DONE]" {
                        break;
                    }

                    if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    // content chunk
                                    if let Some(content_str) = delta.get("content").and_then(|c| c.as_str()) {
                                        if !content_str.is_empty() {
                                            accumulated_content.push_str(content_str);
                                            if let Some(ref tx) = event_tx {
                                                let _ = tx.send(LlmEvent::TextDelta(content_str.to_string()));
                                            }
                                        }
                                    }

                                    // reasoning chunk
                                    let reasoning_str = delta.get("reasoning_content")
                                        .or_else(|| delta.pointer("/choices/0/delta/reasoning"))
                                        .or_else(|| delta.get("reasoning"))
                                        .and_then(|r| r.as_str());

                                    if let Some(r_str) = reasoning_str {
                                        if !r_str.is_empty() {
                                            accumulated_reasoning.push_str(r_str);
                                            if let Some(ref tx) = event_tx {
                                                let _ = tx.send(LlmEvent::Thinking(r_str.to_string()));
                                            }
                                        }
                                    }

                                    // tool_calls chunk
                                    if let Some(tool_calls_arr) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                                        for tc_val in tool_calls_arr {
                                            let index = tc_val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                            if index >= accumulated_tool_calls.len() {
                                                accumulated_tool_calls.resize(index + 1, ToolCallBuilder::default());
                                            }
                                            let builder = &mut accumulated_tool_calls[index];
                                            if let Some(id) = tc_val.get("id").and_then(|i| i.as_str()) {
                                                builder.id = id.to_string();
                                            }
                                            if let Some(name) = tc_val.pointer("/function/name").and_then(|n| n.as_str()) {
                                                builder.name = name.to_string();
                                            }
                                            if let Some(args) = tc_val.pointer("/function/arguments").and_then(|a| a.as_str()) {
                                                builder.arguments.push_str(args);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    event_source.close();
                    return Err(FusionError::Llm(format!("Stream error: {}", e)));
                }
            }
        }

        let final_tool_calls = accumulated_tool_calls
            .into_iter()
            .filter(|b| !b.name.is_empty())
            .map(|b| ToolCall {
                id: b.id,
                name: b.name,
                arguments: serde_json::from_str(&b.arguments)
                    .unwrap_or(serde_json::Value::Object(Default::default())),
            })
            .collect();

        Ok(ChatResult {
            content: accumulated_content,
            reasoning_content: if accumulated_reasoning.is_empty() {
                None
            } else {
                Some(accumulated_reasoning)
            },
            tool_calls: final_tool_calls,
        })
    }
}

#[derive(Default, Clone)]
struct ToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
}

/// Convenience constructor.
pub fn create_llm_client(config: &Config) -> LlmClient {
    LlmClient::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fusion_core::config::{Config, Provider};

    #[tokio::test]
    async fn test_kimi_live() {
        let config = Config {
            model: "@cf/moonshotai/kimi-k2.7-code".to_string(),
            small_model: None,
            yolo: false,
            provider: Provider::Cloudflare,
            cloudflare_account_id: std::env::var("CLOUDFLARE_ACCOUNT_ID").ok(),
            api_key: std::env::var("CLOUDFLARE_API_TOKEN").unwrap_or_default(),
            base_url: String::new(),
            config_path: None,
            settings: std::collections::HashMap::new(),
        };

        let client = LlmClient::new(&config);
        let options = ChatOptions {
            messages: vec![ChatMessage::user("hi")],
            tools: None,
            temperature: Some(0.4),
            max_tokens: Some(100),
        };

        match client.chat(options, None).await {
            Ok(res) => println!("Success: {:?}", res),
            Err(e) => println!("Error: {:?}", e),
        }
    }
}
