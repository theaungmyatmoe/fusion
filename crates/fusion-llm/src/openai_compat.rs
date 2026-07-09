use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
        ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessage, ChatCompletionRequestToolMessage,
        ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessage,
        ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionCall,
    },
    Client,
};
use fusion_core::config::Config;
use fusion_core::error::FusionError;

use crate::client::{ChatOptions, ChatResult, ToolCall};

/// OpenAI-compatible client for xAI, Groq, Together, Ollama, etc.
pub struct OpenAiCompatClient {
    client: Client<OpenAIConfig>,
}

impl OpenAiCompatClient {
    pub fn new(config: &Config) -> Self {
        let oai_config = OpenAIConfig::new()
            .with_api_key(&config.api_key)
            .with_api_base(&config.base_url);

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client: Client::with_config(oai_config).with_http_client(http_client),
        }
    }

    pub async fn chat(
        &self,
        model: &str,
        options: ChatOptions,
    ) -> Result<ChatResult, FusionError> {
        let messages: Vec<ChatCompletionRequestMessage> = options
            .messages
            .iter()
            .map(|m| match m.role.as_str() {
                "system" => ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content: m.content.clone().into(),
                        name: m.name.clone(),
                        ..Default::default()
                    },
                ),
                "user" => {
                    ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                        content: m.content.clone().into(),
                        name: m.name.clone(),
                        ..Default::default()
                    })
                }
                "assistant" => {
                    let tool_calls = m.tool_calls.as_ref().map(|tcs| {
                        tcs.iter()
                            .map(|tc| ChatCompletionMessageToolCall {
                                id: tc.id.clone(),
                                r#type: ChatCompletionToolType::Function,
                                function: FunctionCall {
                                    name: tc.name.clone(),
                                    arguments: tc.arguments.to_string(),
                                },
                            })
                            .collect()
                    });
                    ChatCompletionRequestMessage::Assistant(
                        ChatCompletionRequestAssistantMessage {
                            content: if m.content.is_empty() {
                                None
                            } else {
                                Some(ChatCompletionRequestAssistantMessageContent::Text(m.content.clone()))
                            },
                            tool_calls,
                            name: m.name.clone(),
                            ..Default::default()
                        },
                    )
                }
                "tool" => ChatCompletionRequestMessage::Tool(
                    ChatCompletionRequestToolMessage {
                        content: ChatCompletionRequestToolMessageContent::Text(m.content.clone()),
                        tool_call_id: m.tool_call_id.clone().unwrap_or_default(),
                        ..Default::default()
                    },
                ),
                _ => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                    content: m.content.clone().into(),
                    name: m.name.clone(),
                    ..Default::default()
                }),
            })
            .collect();

        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder.model(model).messages(messages);

        if let Some(temp) = options.temperature {
            req_builder.temperature(temp);
        }
        if let Some(max) = options.max_tokens {
            req_builder.max_tokens(max);
        }

        let request = req_builder
            .build()
            .map_err(|e| FusionError::Llm(format!("Request build error: {}", e)))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| FusionError::Llm(format!("OpenAI-compat error: {}", e)))?;

        let choice = response
            .choices
            .first()
            .ok_or_else(|| FusionError::Llm("No choices in response".into()))?;

        let content = choice
            .message
            .content
            .clone()
            .unwrap_or_default();

        // Parse tool calls if present
        let tool_calls = choice
            .message
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Object(Default::default())),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ChatResult {
            content,
            reasoning_content: None,
            tool_calls,
        })
    }
}
