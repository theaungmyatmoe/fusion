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

/// Client for Cloudflare Workers AI REST API using the OpenAI-compatible v1 endpoint.
pub struct CloudflareClient {
    client: Client<OpenAIConfig>,
    model: String,
    account_id: String,
    api_key: String,
}

impl CloudflareClient {
    pub fn new(config: &Config) -> Self {
        let account_id = config.cloudflare_account_id.clone().unwrap_or_default();
        let api_base = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/v1",
            account_id
        );

        let oai_config = OpenAIConfig::new()
            .with_api_key(&config.api_key)
            .with_api_base(&api_base);

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client: Client::with_config(oai_config).with_http_client(http_client),
            model: config.model.clone(),
            account_id,
            api_key: config.api_key.clone(),
        }
    }

    pub async fn chat(&self, options: ChatOptions) -> Result<ChatResult, FusionError> {
        if self.account_id.is_empty() {
            return Err(FusionError::Llm(
                "CLOUDFLARE_ACCOUNT_ID is required when using Cloudflare Workers AI.\n\
                 Tip: export CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN, or set them in fusion.toml \
                 under [provider.cloudflare]."
                    .into(),
            ));
        }
        if self.api_key.is_empty() {
            return Err(FusionError::Llm(
                "CLOUDFLARE_API_TOKEN is required for Cloudflare Workers AI.\n\
                 Tip: export CLOUDFLARE_API_TOKEN, or set api_key in fusion.toml \
                 under [provider.cloudflare]."
                    .into(),
            ));
        }

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
        req_builder.model(&self.model).messages(messages);

        if let Some(temp) = options.temperature {
            req_builder.temperature(temp);
        }
        if let Some(max) = options.max_tokens {
            req_builder.max_tokens(max);
        }

        // Handle tool definitions
        if let Some(ref tools) = options.tools {
            if let Ok(val) = serde_json::from_value::<Vec<async_openai::types::ChatCompletionTool>>(serde_json::Value::Array(tools.clone())) {
                req_builder.tools(val);
            }
        }

        let request = req_builder
            .build()
            .map_err(|e| FusionError::Llm(format!("Request build error: {}", e)))?;

        // Execute request with retries
        let max_retries = 1;
        let mut last_err = String::new();

        for attempt in 0..=max_retries {
            match self.client.chat().create(request.clone()).await {
                Ok(response) => {
                    let choice = response
                        .choices
                        .first()
                        .ok_or_else(|| FusionError::Llm("No choices in response".into()))?;

                    let content = choice
                        .message
                        .content
                        .clone()
                        .unwrap_or_default();

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

                    return Ok(ChatResult {
                        content,
                        reasoning_content: None,
                        tool_calls,
                    });
                }
                Err(e) => {
                    last_err = format!("Cloudflare API error: {}", e);
                    if attempt < max_retries {
                        let delay_secs = 2u64.pow(attempt + 1);
                        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                    }
                }
            }
        }

        Err(FusionError::Llm(format!(
            "Cloudflare request failed after {} attempts. Error: {}",
            max_retries + 1,
            last_err
        )))
    }
}
