use serde::{Deserialize, Serialize};

use fusion_core::config::{Config, Provider, is_cloudflare_model};
use fusion_core::error::FusionError;

use crate::cloudflare::CloudflareClient;
use crate::openai_compat::OpenAiCompatClient;

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

/// Unified LLM client that routes to the correct provider.
pub struct LlmClient {
    config: Config,
    cloudflare: Option<CloudflareClient>,
    openai_compat: Option<OpenAiCompatClient>,
}

impl LlmClient {
    pub fn new(config: &Config) -> Self {
        let cloudflare = if config.provider == Provider::Cloudflare
            || is_cloudflare_model(&config.model)
        {
            Some(CloudflareClient::new(config))
        } else {
            None
        };

        let openai_compat =
            if config.provider != Provider::Cloudflare && !config.base_url.is_empty() {
                Some(OpenAiCompatClient::new(config))
            } else {
                None
            };

        Self {
            config: config.clone(),
            cloudflare,
            openai_compat,
        }
    }

    /// Send a chat completion request (non-streaming).
    pub async fn chat(&self, mut options: ChatOptions) -> Result<ChatResult, FusionError> {
        if options.max_tokens.is_none() {
            if let Some(info) = fusion_core::models::lookup_model(&self.config.model) {
                options.max_tokens = info.max_tokens_for(fusion_core::models::TokenLevel::Normal);
            } else {
                options.max_tokens = Some(4096);
            }
        }

        if self.config.provider == Provider::Cloudflare
            || is_cloudflare_model(&self.config.model)
        {
            if let Some(ref cf) = self.cloudflare {
                return cf.chat(options).await;
            }
        }

        if let Some(ref compat) = self.openai_compat {
            return compat.chat(&self.config.model, options).await;
        }

        // Fallback: try Cloudflare if we have creds
        if self.config.cloudflare_account_id.is_some() {
            let cf = CloudflareClient::new(&self.config);
            return cf.chat(options).await;
        }

        Err(FusionError::Llm(
            "No valid LLM provider configured. Set FUSION_PROVIDER or provide API keys.".into(),
        ))
    }
}

/// Convenience constructor.
pub fn create_llm_client(config: &Config) -> LlmClient {
    LlmClient::new(config)
}
